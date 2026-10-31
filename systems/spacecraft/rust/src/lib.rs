//! SYS-02 deterministic optical-head availability, not physical pointing.
use ntarlink_contracts::{
    OpticalBodyTrackingSample, OpticalGeometry, OpticalHeadOutage, OpticalPointing,
};
use std::collections::HashMap;

/// Conservative static steering-envelope gate for an operator-supplied
/// off-boresight assumption. This is not an attitude or ephemeris calculation.
pub fn pointing_allowed(pointing: &OpticalPointing) -> bool {
    [
        (pointing.from_off_axis_deg, pointing.from_max_off_axis_deg),
        (pointing.to_off_axis_deg, pointing.to_max_off_axis_deg),
    ]
    .iter()
    .all(|(angle, limit)| {
        angle.is_finite()
            && limit.is_finite()
            && (0.0..=180.0).contains(angle)
            && (0.0..=180.0).contains(limit)
            && angle <= limit
    })
}

/// Compare supplied target and measured boresight directions in each local
/// spacecraft BODY frame. The observation is held between sample instants;
/// this is not a prediction of actuator response or continuous pointing.
pub fn body_tracking_error_allowed(sample: &OpticalBodyTrackingSample) -> bool {
    if sample.frame != "BODY" {
        return false;
    }
    [
        (
            sample.from_target_body,
            sample.from_boresight_body,
            sample.from_max_error_deg,
        ),
        (
            sample.to_target_body,
            sample.to_boresight_body,
            sample.to_max_error_deg,
        ),
    ]
    .iter()
    .all(|(target, actual, limit)| {
        if !limit.is_finite() || !(0.0..=180.0).contains(limit) {
            return false;
        }
        let norm = |v: &[f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        let a = norm(target);
        let b = norm(actual);
        if !a.is_finite() || !b.is_finite() || a <= 0.0 || b <= 0.0 {
            return false;
        }
        let u = target.map(|v| v / a);
        let v = actual.map(|v| v / b);
        let dot = u.iter().zip(v).map(|(x, y)| x * y).sum::<f64>();
        let cross = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        let angle_deg = norm(&cross).atan2(dot).to_degrees();
        angle_deg.is_finite() && angle_deg <= limit + 1.0e-9
    })
}

/// Check whether consecutive commanded BODY-frame target observations are
/// reachable at each exclusive head's configured angular slew rate.
/// This is a finite-sample mean-rate feasibility gate, not inferred boresight
/// or continuous servo tracking. Each endpoint uses its own BODY frame.
pub fn body_tracking_target_rate_allowed(
    previous: &OpticalBodyTrackingSample,
    current: &OpticalBodyTrackingSample,
    from_max_rate_deg_per_s: f64,
    to_max_rate_deg_per_s: f64,
) -> bool {
    if previous.link_id != current.link_id
        || previous.frame != "BODY"
        || current.frame != "BODY"
        || previous.at_ns >= current.at_ns
    {
        return false;
    }
    let elapsed_s = (current.at_ns - previous.at_ns) as f64 * 1.0e-9;
    let within_limit = |a: [f64; 3], b: [f64; 3], rate: f64| {
        if !rate.is_finite() || rate <= 0.0 {
            return false;
        }
        let norm = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        let a_norm = norm(a);
        let b_norm = norm(b);
        if !a_norm.is_finite() || !b_norm.is_finite() || a_norm <= 0.0 || b_norm <= 0.0 {
            return false;
        }
        let a = a.map(|x| x / a_norm);
        let b = b.map(|x| x / b_norm);
        let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        let cross = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let angle_deg = norm(cross).atan2(dot).to_degrees();
        angle_deg.is_finite() && angle_deg <= rate * elapsed_s + 1.0e-9
    };
    within_limit(
        previous.from_target_body,
        current.from_target_body,
        from_max_rate_deg_per_s,
    ) && within_limit(
        previous.to_target_body,
        current.to_target_body,
        to_max_rate_deg_per_s,
    )
}

/// Sampled rest-to-rest target follower for one BODY-frame optical head.
/// The previous command is held over each interval. Velocity resets to zero
/// at sample boundaries; this is not a continuous servo or torque model.
#[derive(Clone, Debug)]
pub struct BodyHeadFollower {
    boresight_body: [f64; 3],
    held_target_body: [f64; 3],
    last_at_ns: u64,
    max_rate_deg_per_s: f64,
    max_acceleration_deg_per_s2: Option<f64>,
}

fn body_unit(vector: [f64; 3]) -> Result<[f64; 3], String> {
    if vector.iter().any(|v| !v.is_finite() || v.abs() > 1.0e12) {
        return Err("invalid BODY-frame direction".into());
    }
    let norm = vector.iter().map(|v| v * v).sum::<f64>().sqrt();
    if !norm.is_finite() || norm <= 0.0 {
        return Err("invalid BODY-frame direction".into());
    }
    Ok(vector.map(|v| v / norm))
}

impl BodyHeadFollower {
    pub fn new(
        initial_direction_body: [f64; 3],
        max_rate_deg_per_s: f64,
        max_acceleration_deg_per_s2: Option<f64>,
    ) -> Result<Self, String> {
        if !max_rate_deg_per_s.is_finite()
            || max_rate_deg_per_s <= 0.0
            || max_acceleration_deg_per_s2.is_some_and(|a| !a.is_finite() || a <= 0.0)
        {
            return Err("invalid BODY-frame follower limits".into());
        }
        let initial = body_unit(initial_direction_body)?;
        Ok(Self {
            boresight_body: initial,
            held_target_body: initial,
            last_at_ns: 0,
            max_rate_deg_per_s,
            max_acceleration_deg_per_s2,
        })
    }

    /// Integrate the previous held command, then latch the new target.
    /// Returns the predicted boresight at the current sample instant.
    pub fn observe(&mut self, at_ns: u64, target_body: [f64; 3]) -> Result<[f64; 3], String> {
        if at_ns < self.last_at_ns {
            return Err("BODY-frame follower timestamps must increase".into());
        }
        let next_target = body_unit(target_body)?;
        let elapsed_s = (at_ns - self.last_at_ns) as f64 * 1.0e-9;
        let max_travel_deg = if let Some(a) = self.max_acceleration_deg_per_s2 {
            let t_to_rate = self.max_rate_deg_per_s / a;
            if elapsed_s <= 2.0 * t_to_rate {
                a * elapsed_s * elapsed_s / 4.0
            } else {
                self.max_rate_deg_per_s * (elapsed_s - t_to_rate)
            }
        } else {
            self.max_rate_deg_per_s * elapsed_s
        };
        if !max_travel_deg.is_finite() {
            return Err("BODY-frame follower travel exceeds numeric range".into());
        }
        let from = self.boresight_body;
        let to = self.held_target_body;
        let dot = from
            .iter()
            .zip(to)
            .map(|(a, b)| a * b)
            .sum::<f64>()
            .clamp(-1.0, 1.0);
        let tangent = std::array::from_fn::<_, 3, _>(|i| to[i] - from[i] * dot);
        let sine = tangent.iter().map(|v| v * v).sum::<f64>().sqrt();
        let angle_deg = sine.atan2(dot).to_degrees();
        let predicted = if angle_deg <= max_travel_deg + 1.0e-12 {
            to
        } else if sine <= 1.0e-12 {
            // Exact antiparallel directions have no unique shortest-arc plane.
            return Err("ambiguous antiparallel BODY-frame follower target".into());
        } else {
            let step = max_travel_deg.to_radians();
            let rotated = std::array::from_fn::<_, 3, _>(|i| {
                from[i] * step.cos() + tangent[i] / sine * step.sin()
            });
            body_unit(rotated)?
        };
        self.boresight_body = predicted;
        self.held_target_body = next_target;
        self.last_at_ns = at_ns;
        Ok(predicted)
    }

    /// Project a head to a PAT boundary without changing its sampled command history.
    /// The projection retains the last commanded target and rest-to-rest model.
    pub fn predicted_at(&self, at_ns: u64) -> Result<([f64; 3], [f64; 3]), String> {
        let mut projection = self.clone();
        let target = projection.held_target_body;
        let boresight = projection.observe(at_ns, target)?;
        Ok((boresight, target))
    }
}

/// Compute the two line-of-sight steering angles from one ITRS snapshot.
/// Boresight and position vectors must share the ITRS axes; a fixed snapshot
/// is not a time-varying attitude or ephemeris model. A degenerate vector
/// fails closed. The numerical comparison tolerance is 1e-9 degrees.
pub fn geometry_pointing_allowed(geometry: &OpticalGeometry) -> bool {
    if geometry.frame != "ITRS" {
        return false;
    }
    let delta =
        std::array::from_fn::<_, 3, _>(|i| geometry.to_position_m[i] - geometry.from_position_m[i]);
    let reverse = delta.map(|v| -v);
    [
        (
            delta,
            geometry.from_boresight_itrs,
            geometry.from_max_off_axis_deg,
        ),
        (
            reverse,
            geometry.to_boresight_itrs,
            geometry.to_max_off_axis_deg,
        ),
    ]
    .iter()
    .all(|(direction, boresight, limit)| {
        if !limit.is_finite() || !(0.0..=180.0).contains(limit) {
            return false;
        }
        let norm = |v: &[f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        let a = norm(direction);
        let b = norm(boresight);
        if !a.is_finite() || !b.is_finite() || a <= 0.0 || b <= 0.0 {
            return false;
        }
        let dot = direction
            .iter()
            .zip(boresight)
            .map(|(x, y)| (x / a) * (y / b))
            .sum::<f64>();
        dot.is_finite() && dot.clamp(-1.0, 1.0).acos().to_degrees() <= limit + 1.0e-9
    })
}

/// Check the angular displacement of the required ITRS LOS between observations.
/// The sampled rate is compared with the configured bound over the elapsed
/// virtual time. This cannot certify unsampled motion or body-frame actuator
/// tracking; both endpoints share the same ITRS LOS angular displacement.
pub fn geometry_los_rate_allowed(
    previous: &OpticalGeometry,
    current: &OpticalGeometry,
    max_rate_deg_per_s: f64,
) -> bool {
    if current.at_ns <= previous.at_ns
        || !max_rate_deg_per_s.is_finite()
        || max_rate_deg_per_s < 0.0
        || previous.frame != "ITRS"
        || current.frame != "ITRS"
    {
        return false;
    }
    let direction = |sample: &OpticalGeometry| {
        std::array::from_fn::<_, 3, _>(|i| sample.to_position_m[i] - sample.from_position_m[i])
    };
    let a = direction(previous);
    let b = direction(current);
    let norm = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    let a_norm = norm(a);
    let b_norm = norm(b);
    if !a_norm.is_finite() || !b_norm.is_finite() || a_norm <= 0.0 || b_norm <= 0.0 {
        return false;
    }
    let a = a.map(|x| x / a_norm);
    let b = b.map(|x| x / b_norm);
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
    let cross = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let angle_deg = norm(cross).atan2(dot).to_degrees();
    let elapsed_s = (current.at_ns - previous.at_ns) as f64 * 1.0e-9;
    angle_deg.is_finite() && angle_deg <= max_rate_deg_per_s * elapsed_s + 1.0e-9
}

/// Finite-difference ITRS LOS angular acceleration at the third observation.
/// The signed rotation vectors detect direction reversals that scalar angular
/// speed differences miss. An exact antiparallel LOS has no unique rotation
/// axis and fails closed. This is not body-frame actuator acceleration.
pub fn geometry_los_acceleration_allowed(
    first: &OpticalGeometry,
    second: &OpticalGeometry,
    third: &OpticalGeometry,
    max_acceleration_deg_per_s2: f64,
) -> bool {
    if !max_acceleration_deg_per_s2.is_finite()
        || max_acceleration_deg_per_s2 < 0.0
        || first.link_id != second.link_id
        || second.link_id != third.link_id
        || first.at_ns >= second.at_ns
        || second.at_ns >= third.at_ns
    {
        return false;
    }

    fn angular_velocity(a: &OpticalGeometry, b: &OpticalGeometry) -> Option<[f64; 3]> {
        if a.frame != "ITRS" || b.frame != "ITRS" {
            return None;
        }
        let los = |sample: &OpticalGeometry| {
            std::array::from_fn::<_, 3, _>(|i| sample.to_position_m[i] - sample.from_position_m[i])
        };
        let norm = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
        let elapsed_s = (b.at_ns - a.at_ns) as f64 * 1.0e-9;
        let a = los(a);
        let b = los(b);
        let a_norm = norm(a);
        let b_norm = norm(b);
        if !a_norm.is_finite() || !b_norm.is_finite() || a_norm <= 0.0 || b_norm <= 0.0 {
            return None;
        }
        let a = a.map(|v| v / a_norm);
        let b = b.map(|v| v / b_norm);
        let cross = [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ];
        let sine = norm(cross);
        let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        if !sine.is_finite() || !dot.is_finite() || (dot < 0.0 && sine < 1.0e-12) {
            return None;
        }
        if sine == 0.0 {
            return Some([0.0; 3]);
        }
        let scale = sine.atan2(dot).to_degrees() / (sine * elapsed_s);
        let velocity = cross.map(|component| component * scale);
        velocity
            .iter()
            .all(|value| value.is_finite())
            .then_some(velocity)
    }

    let (Some(before), Some(after)) = (
        angular_velocity(first, second),
        angular_velocity(second, third),
    ) else {
        return false;
    };
    let midpoint_seconds =
        ((second.at_ns - first.at_ns) as f64 + (third.at_ns - second.at_ns) as f64) * 0.5e-9;
    let acceleration = before
        .iter()
        .zip(after)
        .map(|(a, b)| (b - a) * (b - a))
        .sum::<f64>()
        .sqrt()
        / midpoint_seconds;
    acceleration.is_finite() && acceleration <= max_acceleration_deg_per_s2 + 1.0e-6
}

/// Indexed half-open outage windows for spacecraft-owned optical heads.
/// Scenario validation rejects overlaps and unknown head ownership.
#[derive(Default)]
pub struct HeadAvailability {
    blocked: HashMap<String, Vec<(u64, u64)>>,
}

impl HeadAvailability {
    pub fn new(outages: &[OpticalHeadOutage]) -> Self {
        let mut blocked: HashMap<String, Vec<(u64, u64)>> = HashMap::new();
        for outage in outages {
            blocked
                .entry(outage.head_id.clone())
                .or_default()
                .push((outage.start_ns, outage.end_ns));
        }
        for intervals in blocked.values_mut() {
            intervals.sort_unstable();
        }
        Self { blocked }
    }

    /// Return disjoint half-open intervals when both optical heads are available.
    /// Intersections are computed from indexed outages, never from a time-step scan.
    pub fn available_intervals(
        &self,
        from_head: &str,
        to_head: &str,
        start_ns: u64,
        end_ns: u64,
    ) -> Vec<(u64, u64)> {
        if start_ns >= end_ns {
            return Vec::new();
        }
        let mut blocked = Vec::new();
        for head in [from_head, to_head] {
            if let Some(intervals) = self.blocked.get(head) {
                let first = intervals.partition_point(|(_, end)| *end <= start_ns);
                for &(start, end) in &intervals[first..] {
                    if start >= end_ns {
                        break;
                    }
                    blocked.push((start.max(start_ns), end.min(end_ns)));
                }
            }
        }
        blocked.sort_unstable();
        let mut available = Vec::new();
        let mut cursor = start_ns;
        for (start, end) in blocked {
            if cursor < start {
                available.push((cursor, start));
            }
            cursor = cursor.max(end);
        }
        if cursor < end_ns {
            available.push((cursor, end_ns));
        }
        available
    }

    pub fn outage_starts_at(&self, head: &str, at_ns: u64) -> bool {
        self.blocked.get(head).is_some_and(|intervals| {
            intervals
                .binary_search_by_key(&at_ns, |(start, _)| *start)
                .is_ok()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn outage(head: &str, start: u64, end: u64) -> OpticalHeadOutage {
        OpticalHeadOutage {
            head_id: head.into(),
            start_ns: start,
            end_ns: end,
        }
    }

    #[test]
    fn static_pointing_gate_is_inclusive_and_checks_both_heads() {
        let mut pointing = OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 30.0,
            to_off_axis_deg: 20.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 20.0,
        };
        assert!(pointing_allowed(&pointing));
        pointing.to_off_axis_deg = 20.01;
        assert!(!pointing_allowed(&pointing));
        pointing.to_off_axis_deg = 20.0;
        pointing.from_off_axis_deg = 30.01;
        assert!(!pointing_allowed(&pointing));
        pointing.from_off_axis_deg = f64::NAN;
        assert!(!pointing_allowed(&pointing));
    }

    #[test]
    fn body_tracking_error_is_reciprocal_frame_local_and_fails_closed() {
        let mut sample = OpticalBodyTrackingSample {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            frame: "BODY".into(),
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [-1.0, 0.0, 0.0],
            from_boresight_body: [2.0, 0.0, 0.0],
            to_boresight_body: [-2.0, 0.0, 0.0],
            from_max_error_deg: 5.0,
            to_max_error_deg: 5.0,
        };
        assert!(body_tracking_error_allowed(&sample));
        sample.from_boresight_body = [0.0, 1.0, 0.0];
        assert!(!body_tracking_error_allowed(&sample));
        sample.from_max_error_deg = 90.0;
        assert!(body_tracking_error_allowed(&sample));
        sample.to_boresight_body = [1.0, 0.0, 0.0];
        assert!(!body_tracking_error_allowed(&sample));
        sample.to_max_error_deg = 180.0;
        assert!(body_tracking_error_allowed(&sample));
        sample.from_boresight_body = [0.0; 3];
        assert!(!body_tracking_error_allowed(&sample));
        sample.from_boresight_body = [1.0, 0.0, 0.0];
        sample.from_max_error_deg = f64::NAN;
        assert!(!body_tracking_error_allowed(&sample));
        sample.from_max_error_deg = 5.0;
        sample.frame = "ITRS".into();
        assert!(!body_tracking_error_allowed(&sample));
    }

    #[test]
    fn body_tracking_target_rate_is_reciprocal_and_uses_elapsed_virtual_time() {
        let previous = OpticalBodyTrackingSample {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            frame: "BODY".into(),
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [-1.0, 0.0, 0.0],
            from_boresight_body: [1.0, 0.0, 0.0],
            to_boresight_body: [-1.0, 0.0, 0.0],
            from_max_error_deg: 5.0,
            to_max_error_deg: 5.0,
        };
        let mut next = previous.clone();
        next.at_ns = 25_000_000;
        next.from_target_body = [0.0, 1.0, 0.0];
        next.from_boresight_body = [0.0, 1.0, 0.0];
        assert!(body_tracking_error_allowed(&next));
        assert!(!body_tracking_target_rate_allowed(
            &previous, &next, 3000.0, 3000.0
        ));
        next.at_ns = 30_000_000;
        assert!(body_tracking_target_rate_allowed(
            &previous, &next, 3000.0, 3000.0
        ));
        next.to_target_body = [0.0, -1.0, 0.0];
        assert!(!body_tracking_target_rate_allowed(
            &previous, &next, 3000.0, 2000.0
        ));
        next.to_target_body = previous.to_target_body;
        next.at_ns = 0;
        assert!(!body_tracking_target_rate_allowed(
            &previous, &next, 3000.0, 3000.0
        ));
        next.at_ns = 30_000_000;
        assert!(!body_tracking_target_rate_allowed(
            &previous,
            &next,
            f64::NAN,
            3000.0
        ));
        next.from_target_body = [0.0; 3];
        assert!(!body_tracking_target_rate_allowed(
            &previous, &next, 3000.0, 3000.0
        ));
    }

    #[test]
    fn sampled_follower_predicts_lag_and_acceleration_limited_recovery() {
        let mut head = BodyHeadFollower::new([1.0, 0.0, 0.0], 3000.0, Some(200000.0)).unwrap();
        assert_eq!(head.observe(0, [1.0, 0.0, 0.0]).unwrap(), [1.0, 0.0, 0.0]);
        assert_eq!(
            head.observe(30_000_000, [0.0, 1.0, 0.0]).unwrap(),
            [1.0, 0.0, 0.0]
        );
        let halfway = head.observe(60_000_000, [0.0, 1.0, 0.0]).unwrap();
        assert!((halfway[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1.0e-12);
        assert!((halfway[1] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1.0e-12);
        let reached = head.observe(90_000_000, [0.0, 1.0, 0.0]).unwrap();
        assert!(reached[0].abs() < 1.0e-12 && (reached[1] - 1.0).abs() < 1.0e-12);
        assert!(head.observe(89_000_000, [0.0, 1.0, 0.0]).is_err());
    }

    #[test]
    fn follower_projection_preserves_sample_history_and_held_target() {
        let mut head = BodyHeadFollower::new([1.0, 0.0, 0.0], 3000.0, Some(200000.0)).unwrap();
        head.observe(0, [1.0, 0.0, 0.0]).unwrap();
        head.observe(30_000_000, [0.0, 1.0, 0.0]).unwrap();
        let (projected, target) = head.predicted_at(60_000_000).unwrap();
        assert_eq!(target, [0.0, 1.0, 0.0]);
        assert!((projected[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1.0e-12);
        assert_eq!(head.observe(60_000_000, target).unwrap(), projected);
        assert!(head.predicted_at(59_000_000).is_err());
    }

    #[test]
    fn sampled_follower_rejects_invalid_and_ambiguous_commands() {
        assert!(BodyHeadFollower::new([0.0; 3], 1.0, None).is_err());
        assert!(BodyHeadFollower::new([1.0, 0.0, 0.0], 0.0, None).is_err());
        let mut head = BodyHeadFollower::new([1.0, 0.0, 0.0], 100.0, None).unwrap();
        head.observe(0, [-1.0, 0.0, 0.0]).unwrap();
        assert!(head.observe(10_000_000, [-1.0, 0.0, 0.0]).is_err());
        assert!(head.observe(10_000_000, [f64::NAN, 0.0, 0.0]).is_err());
    }

    #[test]
    fn itrs_geometry_checks_both_reciprocal_los_directions() {
        let mut g = OpticalGeometry {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            frame: "ITRS".into(),
            from_position_m: [7_000_000.0, 0.0, 0.0],
            to_position_m: [7_001_000.0, 0.0, 0.0],
            from_boresight_itrs: [1.0, 0.0, 0.0],
            to_boresight_itrs: [-1.0, 0.0, 0.0],
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        };
        assert!(geometry_pointing_allowed(&g));
        g.to_boresight_itrs = [0.0, 1.0, 0.0];
        assert!(!geometry_pointing_allowed(&g));
        g.to_max_off_axis_deg = 90.0;
        assert!(geometry_pointing_allowed(&g));
        g.from_boresight_itrs = [0.0, 0.0, 0.0];
        assert!(!geometry_pointing_allowed(&g));
        g.from_boresight_itrs = [1.0, 0.0, 0.0];
        g.to_position_m = g.from_position_m;
        assert!(!geometry_pointing_allowed(&g));
        g.to_position_m = [7_001_000.0, 0.0, 0.0];
        g.frame = "TEME".into();
        assert!(!geometry_pointing_allowed(&g));
    }

    #[test]
    fn sampled_itrs_los_rate_uses_elapsed_time_and_fails_closed() {
        let initial = OpticalGeometry {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            frame: "ITRS".into(),
            from_position_m: [7_000_000.0, 0.0, 0.0],
            to_position_m: [7_001_000.0, 0.0, 0.0],
            from_boresight_itrs: [1.0, 0.0, 0.0],
            to_boresight_itrs: [-1.0, 0.0, 0.0],
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        };
        let mut rotated = initial.clone();
        rotated.at_ns = 30_000_000;
        rotated.to_position_m = [7_000_000.0, 1000.0, 0.0];
        assert!(!geometry_los_rate_allowed(&initial, &rotated, 1000.0));
        assert!(geometry_los_rate_allowed(&initial, &rotated, 3000.001));
        assert!(!geometry_los_rate_allowed(&initial, &rotated, f64::NAN));
        assert!(!geometry_los_rate_allowed(&initial, &rotated, -1.0));
        rotated.at_ns = 0;
        assert!(!geometry_los_rate_allowed(&initial, &rotated, 5000.0));
        rotated.at_ns = 30_000_000;
        rotated.to_position_m = initial.to_position_m;
        assert!(geometry_los_rate_allowed(&initial, &rotated, 0.0));
        rotated.to_position_m = initial.from_position_m;
        assert!(!geometry_los_rate_allowed(&initial, &rotated, 5000.0));
    }

    #[test]
    fn sampled_itrs_los_acceleration_detects_steps_reversal_and_ambiguity() {
        let initial = OpticalGeometry {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            frame: "ITRS".into(),
            from_position_m: [7_000_000.0, 0.0, 0.0],
            to_position_m: [7_001_000.0, 0.0, 0.0],
            from_boresight_itrs: [1.0, 0.0, 0.0],
            to_boresight_itrs: [-1.0, 0.0, 0.0],
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        };
        let mut second = initial.clone();
        second.at_ns = 30_000_000;
        second.to_position_m = [7_000_000.0, 1000.0, 0.0];
        let mut third = second.clone();
        third.at_ns = 50_000_000;
        assert!(!geometry_los_acceleration_allowed(
            &initial, &second, &third, 100_000.0
        ));
        assert!(geometry_los_acceleration_allowed(
            &initial, &second, &third, 120_001.0
        ));
        assert!(!geometry_los_acceleration_allowed(
            &initial,
            &second,
            &third,
            f64::NAN
        ));
        assert!(!geometry_los_acceleration_allowed(
            &initial, &second, &third, -1.0
        ));
        third.at_ns = 60_000_000;
        third.to_position_m = initial.to_position_m;
        assert!(!geometry_los_acceleration_allowed(
            &initial, &second, &third, 150_000.0
        ));
        assert!(geometry_los_acceleration_allowed(
            &initial, &second, &third, 200_001.0
        ));
        second.to_position_m = [6_999_000.0, 0.0, 0.0];
        assert!(!geometry_los_acceleration_allowed(
            &initial, &second, &third, 1.0e9
        ));
        second.to_position_m = initial.to_position_m;
        third.to_position_m = initial.to_position_m;
        assert!(geometry_los_acceleration_allowed(
            &initial, &second, &third, 0.0
        ));
        third.to_position_m = third.from_position_m;
        assert!(!geometry_los_acceleration_allowed(
            &initial, &second, &third, 1.0e9
        ));
    }

    #[test]
    fn splits_at_either_head_and_reopens_at_exclusive_end() {
        let availability = HeadAvailability::new(&[
            outage("a", 20, 40),
            outage("b", 35, 60),
            outage("a", 80, 90),
        ]);
        assert_eq!(
            availability.available_intervals("a", "b", 0, 100),
            vec![(0, 20), (60, 80), (90, 100)]
        );
        assert!(availability.outage_starts_at("a", 20));
        assert!(!availability.outage_starts_at("a", 40));
    }

    #[test]
    fn clips_to_contact_and_ignores_unrelated_heads() {
        let availability = HeadAvailability::new(&[
            outage("a", 0, 50),
            outage("c", 50, 90),
            outage("b", 90, 120),
        ]);
        assert_eq!(
            availability.available_intervals("a", "b", 30, 100),
            vec![(50, 90)]
        );
        assert_eq!(
            availability.available_intervals("x", "y", 30, 100),
            vec![(30, 100)]
        );
        assert!(availability
            .available_intervals("a", "b", 50, 50)
            .is_empty());
    }

    #[test]
    fn input_order_does_not_change_availability() {
        let a = outage("a", 20, 40);
        let b = outage("b", 60, 80);
        let forward = HeadAvailability::new(&[a.clone(), b.clone()]);
        let reverse = HeadAvailability::new(&[b, a]);
        assert_eq!(
            forward.available_intervals("a", "b", 0, 100),
            reverse.available_intervals("a", "b", 0, 100)
        );
    }
}
