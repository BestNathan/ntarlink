//! SYS-02 deterministic optical-head availability, not physical pointing.
use ntarlink_contracts::{OpticalGeometry, OpticalHeadOutage, OpticalPointing};
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
            std::array::from_fn::<_, 3, _>(|i| {
                sample.to_position_m[i] - sample.from_position_m[i]
            })
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
        velocity.iter().all(|value| value.is_finite()).then_some(velocity)
    }

    let (Some(before), Some(after)) = (
        angular_velocity(first, second),
        angular_velocity(second, third),
    ) else {
        return false;
    };
    let midpoint_seconds = ((second.at_ns - first.at_ns) as f64
        + (third.at_ns - second.at_ns) as f64)
        * 0.5e-9;
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
            &initial, &second, &third, f64::NAN
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
