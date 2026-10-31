//! SYS-03 deterministic optical terminal acquisition scheduling.
//!
//! This is an offline virtual-time planning primitive. Geometric candidates
//! come from SYS-01; neither a contact nor this schedule is an optical PHY
//! measurement or a packet-runtime admission decision.

use std::collections::{HashMap, HashSet};

pub const MAX_CANDIDATES: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatCandidate {
    pub link_id: String,
    pub from_head: String,
    pub to_head: String,
    /// Half-open geometric visibility interval, in scenario-relative nanoseconds.
    pub start_ns: u64,
    pub end_ns: u64,
    /// Assumed deterministic optical terminal slew and acquisition dwell times.
    pub slew_ns: u64,
    pub acquisition_ns: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatState {
    Candidate,
    Slewing,
    Acquiring,
    Tracking,
    Lost,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatEvent {
    pub link_id: String,
    pub at_ns: u64,
    pub state: PatState,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcquiredWindow {
    pub link_id: String,
    /// First instant when both heads have completed acquisition.
    pub start_ns: u64,
    /// Exclusive end; the optical link is unavailable at this instant.
    pub end_ns: u64,
}

/// Exclusive occupancy of both optical heads, including slew and acquisition.
/// The half-open interval starts when both heads become available, not when
/// the geometric candidate was first observed. Rejected contacts reserve none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatReservation {
    pub link_id: String,
    pub from_head: String,
    pub to_head: String,
    pub start_ns: u64,
    pub end_ns: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatPlan {
    pub windows: Vec<AcquiredWindow>,
    /// Successful exclusive head reservations, including pre-tracking dwell.
    pub reservations: Vec<PatReservation>,
    pub events: Vec<PatEvent>,
}

/// Canonical half-open PAT lease boundary, ordered by virtual time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatLeaseTransitionKind {
    Release,
    Reserve,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatLeaseTransition {
    pub at_ns: u64,
    pub kind: PatLeaseTransitionKind,
    /// Index into the caller's reservation slice.
    pub reservation_index: usize,
}

/// Preflight all accepted reservations before mutating SYS-02 command leases.
/// Releases precede reservations at equal times, permitting exact handoff.
/// Invalid intervals, duplicate active links and overlapping heads fail closed.
/// This is offline O(A log A) validation, not live arbitration.
pub fn reservation_transitions(
    reservations: &[PatReservation],
) -> Result<Vec<PatLeaseTransition>, String> {
    if reservations.len() > MAX_CANDIDATES {
        return Err("PAT reservation budget exceeded".into());
    }
    let mut timeline = Vec::with_capacity(reservations.len() * 2);
    for (index, reservation) in reservations.iter().enumerate() {
        if reservation.link_id.trim().is_empty()
            || reservation.from_head.trim().is_empty()
            || reservation.to_head.trim().is_empty()
            || reservation.from_head == reservation.to_head
            || reservation.start_ns >= reservation.end_ns
        {
            return Err("invalid PAT head reservation interval or identifiers".into());
        }
        timeline.push(PatLeaseTransition {
            at_ns: reservation.start_ns,
            kind: PatLeaseTransitionKind::Reserve,
            reservation_index: index,
        });
        timeline.push(PatLeaseTransition {
            at_ns: reservation.end_ns,
            kind: PatLeaseTransitionKind::Release,
            reservation_index: index,
        });
    }
    let order = |event: &PatLeaseTransition| {
        (
            event.at_ns,
            match event.kind {
                PatLeaseTransitionKind::Release => 0_u8,
                PatLeaseTransitionKind::Reserve => 1_u8,
            },
            reservations[event.reservation_index].link_id.as_str(),
            event.reservation_index,
        )
    };
    timeline.sort_by(|a, b| order(a).cmp(&order(b)));

    let mut active_links: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut head_owners: HashMap<&str, &str> = HashMap::new();
    for event in &timeline {
        let reservation = &reservations[event.reservation_index];
        let link = reservation.link_id.as_str();
        let from = reservation.from_head.as_str();
        let to = reservation.to_head.as_str();
        match event.kind {
            PatLeaseTransitionKind::Reserve => {
                if active_links.contains_key(link)
                    || head_owners.contains_key(from)
                    || head_owners.contains_key(to)
                {
                    return Err("overlapping PAT head reservations".into());
                }
                active_links.insert(link, (from, to));
                head_owners.insert(from, link);
                head_owners.insert(to, link);
            }
            PatLeaseTransitionKind::Release => {
                if active_links.get(link).copied() != Some((from, to))
                    || head_owners.get(from).copied() != Some(link)
                    || head_owners.get(to).copied() != Some(link)
                {
                    return Err("PAT reservation release ownership mismatch".into());
                }
                active_links.remove(link);
                head_owners.remove(from);
                head_owners.remove(to);
            }
        }
    }
    Ok(timeline)
}

/// Typed virtual-time transitions for PAT leases and sampled head commands.
/// The packet adapter supplies SYS-02 samples without moving command ownership
/// into SYS-03. This is still an offline schedule, not a live actuator loop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatRuntimeTransitionKind {
    Release(usize),
    Reserve(usize),
    Sample(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PatRuntimeTransition {
    pub at_ns: u64,
    pub kind: PatRuntimeTransitionKind,
}

/// Merge validated PAT reservation edges with sampled SYS-02 commands.
/// At the same instant, release precedes reserve, which precedes sample.
/// Link IDs and original indices make the ordering deterministic; duplicate
/// samples for a link at one instant are rejected before any lease mutation.
pub fn pat_runtime_transitions(
    reservations: &[PatReservation],
    samples: &[(u64, &str)],
) -> Result<Vec<PatRuntimeTransition>, String> {
    if samples.len() > MAX_CANDIDATES {
        return Err("PAT sampled command budget exceeded".into());
    }
    let lease_edges = reservation_transitions(reservations)?;
    let mut events = Vec::with_capacity(lease_edges.len() + samples.len());
    for edge in lease_edges {
        let reservation = &reservations[edge.reservation_index];
        let (priority, kind) = match edge.kind {
            PatLeaseTransitionKind::Release => (
                0_u8,
                PatRuntimeTransitionKind::Release(edge.reservation_index),
            ),
            PatLeaseTransitionKind::Reserve => (
                1_u8,
                PatRuntimeTransitionKind::Reserve(edge.reservation_index),
            ),
        };
        events.push((
            edge.at_ns,
            priority,
            reservation.link_id.as_str(),
            edge.reservation_index,
            kind,
        ));
    }
    let mut seen = HashSet::new();
    for (index, &(at_ns, link_id)) in samples.iter().enumerate() {
        if link_id.trim().is_empty() || !seen.insert((link_id, at_ns)) {
            return Err("invalid or duplicate PAT sampled head command".into());
        }
        events.push((
            at_ns,
            2_u8,
            link_id,
            index,
            PatRuntimeTransitionKind::Sample(index),
        ));
    }
    events.sort_by(|a, b| (a.0, a.1, a.2, a.3).cmp(&(b.0, b.1, b.2, b.3)));
    Ok(events
        .into_iter()
        .map(|(at_ns, _, _, _, kind)| PatRuntimeTransition { at_ns, kind })
        .collect())
}

/// Constant-rate body-frame gimbal slew assumption for one exclusive head.
/// This is a sampled actuator travel-time model, not a servo or torque model.
#[derive(Clone, Debug, PartialEq)]
pub struct HeadSlewActuator {
    pub head_id: String,
    pub initial_direction_body: [f64; 3],
    pub max_slew_rate_deg_per_s: f64,
    /// Optional rest-to-rest angular acceleration limit in degrees per second squared.
    pub max_slew_acceleration_deg_per_s2: Option<f64>,
    /// Optional post-motion settling dwell; applied only after nonzero retargeting.
    pub settling_ns: Option<u64>,
}

/// Fixed body-frame target for one geometric contact or recovered segment.
#[derive(Clone, Debug, PartialEq)]
pub struct PatSlewTarget {
    pub link_id: String,
    pub start_ns: u64,
    pub from_target_body: [f64; 3],
    pub to_target_body: [f64; 3],
}

/// Predicted head pointing at a PAT candidate boundary, in each head's BODY frame.
#[derive(Clone, Debug, PartialEq)]
pub struct PatSlewStart {
    pub link_id: String,
    pub start_ns: u64,
    pub from_direction_body: [f64; 3],
    pub to_direction_body: [f64; 3],
}

fn unit_direction(vector: [f64; 3]) -> Option<[f64; 3]> {
    if vector
        .iter()
        .any(|value| !value.is_finite() || value.abs() > 1.0e12)
    {
        return None;
    }
    let norm = vector.iter().map(|value| value * value).sum::<f64>().sqrt();
    (norm.is_finite() && norm > 0.0).then(|| vector.map(|value| value / norm))
}

fn slew_time_ns(
    from: [f64; 3],
    to: [f64; 3],
    rate_deg_per_s: f64,
    acceleration_deg_per_s2: Option<f64>,
) -> Result<u64, String> {
    let from = unit_direction(from).ok_or("invalid initial body-frame gimbal direction")?;
    let to = unit_direction(to).ok_or("invalid body-frame gimbal target")?;
    if !rate_deg_per_s.is_finite() || rate_deg_per_s <= 0.0 {
        return Err("gimbal slew rate must be finite and positive".into());
    }
    let cross = [
        from[1] * to[2] - from[2] * to[1],
        from[2] * to[0] - from[0] * to[2],
        from[0] * to[1] - from[1] * to[0],
    ];
    let sine = cross.iter().map(|v| v * v).sum::<f64>().sqrt();
    let cosine = from.iter().zip(to).map(|(a, b)| a * b).sum::<f64>();
    let angle_deg = sine.atan2(cosine).to_degrees();
    // Symmetric rest-to-rest bang-bang profile: triangular below the speed
    // saturation distance, trapezoidal otherwise. No torque or servo model.
    let travel_s = if let Some(acceleration) = acceleration_deg_per_s2 {
        if !acceleration.is_finite() || acceleration <= 0.0 {
            return Err("gimbal slew acceleration must be finite and positive".into());
        }
        let saturation_angle = rate_deg_per_s * (rate_deg_per_s / acceleration);
        if angle_deg <= saturation_angle {
            2.0 * (angle_deg / acceleration).sqrt()
        } else {
            angle_deg / rate_deg_per_s + rate_deg_per_s / acceleration
        }
    } else {
        angle_deg / rate_deg_per_s
    };
    let time_ns = (travel_s * 1.0e9).ceil();
    if !time_ns.is_finite() || time_ns >= u64::MAX as f64 {
        return Err("gimbal slew time exceeds virtual-time range".into());
    }
    Ok(time_ns as u64)
}

fn slew_and_settle_ns(
    from: [f64; 3],
    to: [f64; 3],
    rate_deg_per_s: f64,
    acceleration_deg_per_s2: Option<f64>,
    settling_ns: u64,
) -> Result<u64, String> {
    let travel_ns = slew_time_ns(from, to, rate_deg_per_s, acceleration_deg_per_s2)?;
    if travel_ns == 0 {
        return Ok(0);
    }
    travel_ns
        .checked_add(settling_ns)
        .ok_or_else(|| "gimbal slew and settling duration overflow".into())
}

fn state_order(state: PatState) -> u8 {
    match state {
        PatState::Lost => 0,
        PatState::Candidate => 1,
        PatState::Slewing => 2,
        PatState::Acquiring => 3,
        PatState::Tracking => 4,
        PatState::Rejected => 5,
    }
}

/// Schedule nonpreemptive, exclusive optical-head reservations.
///
/// Candidates are processed by geometric start time and link ID, regardless
/// of caller order. A head remains reserved through the contact's exclusive
/// end once a link is acquired. Conflicts wait for both heads to be free;
/// candidates with insufficient remaining dwell time are rejected. This
/// models fixed delays, not pointing dynamics, tracking loss or link budget.
pub fn plan_contacts(candidates: &[PatCandidate]) -> Result<PatPlan, String> {
    plan_contacts_with_slew(candidates, &[], &[])
}

/// Plan contacts with optional per-head constant-rate body-frame slew.
/// Target observations are tied to the *candidate* start, including segments
/// created after a fault. Rejected contacts do not move either actuator.
pub fn plan_contacts_with_slew(
    candidates: &[PatCandidate],
    actuators: &[HeadSlewActuator],
    targets: &[PatSlewTarget],
) -> Result<PatPlan, String> {
    plan_contacts_with_slew_starts(candidates, actuators, targets, &[])
}

/// Accept predicted per-candidate pointing from SYS-02 rather than inferring
/// the start direction from a previous PAT contact's static target.
/// The snapshot set must cover all candidates when supplied.
pub fn plan_contacts_with_slew_starts(
    candidates: &[PatCandidate],
    actuators: &[HeadSlewActuator],
    targets: &[PatSlewTarget],
    starts: &[PatSlewStart],
) -> Result<PatPlan, String> {
    if (actuators.is_empty() && !targets.is_empty())
        || (!actuators.is_empty() && targets.is_empty() && !candidates.is_empty())
    {
        return Err("gimbal actuators and contact targets must be configured together".into());
    }
    let mut head_rates = HashMap::new();
    let mut head_directions = HashMap::new();
    for actuator in actuators {
        if actuator.head_id.trim().is_empty()
            || head_rates
                .insert(
                    actuator.head_id.clone(),
                    (
                        actuator.max_slew_rate_deg_per_s,
                        actuator.max_slew_acceleration_deg_per_s2,
                        actuator.settling_ns,
                    ),
                )
                .is_some()
            || !actuator.max_slew_rate_deg_per_s.is_finite()
            || actuator.max_slew_rate_deg_per_s <= 0.0
            || actuator
                .max_slew_acceleration_deg_per_s2
                .is_some_and(|accel| !accel.is_finite() || accel <= 0.0)
            || unit_direction(actuator.initial_direction_body).is_none()
        {
            return Err("invalid or duplicate body-frame gimbal actuator".into());
        }
        head_directions.insert(actuator.head_id.clone(), actuator.initial_direction_body);
    }
    let mut target_by_candidate = HashMap::new();
    for target in targets {
        if target.link_id.trim().is_empty()
            || unit_direction(target.from_target_body).is_none()
            || unit_direction(target.to_target_body).is_none()
            || target_by_candidate
                .insert((target.link_id.as_str(), target.start_ns), target)
                .is_some()
        {
            return Err("invalid or duplicate body-frame PAT target".into());
        }
    }
    let mut start_by_candidate = HashMap::new();
    for start in starts {
        if unit_direction(start.from_direction_body).is_none()
            || unit_direction(start.to_direction_body).is_none()
            || start_by_candidate
                .insert((start.link_id.as_str(), start.start_ns), start)
                .is_some()
        {
            return Err("invalid or duplicate predicted PAT start pointing".into());
        }
    }
    if !starts.is_empty()
        && (actuators.is_empty()
            || starts.len() != candidates.len()
            || candidates.iter().any(|candidate| {
                !start_by_candidate.contains_key(&(candidate.link_id.as_str(), candidate.start_ns))
            }))
    {
        return Err("predicted PAT start pointing must cover every candidate".into());
    }
    if !actuators.is_empty()
        && (targets.len() != candidates.len()
            || candidates.iter().any(|candidate| {
                !head_rates.contains_key(&candidate.from_head)
                    || !head_rates.contains_key(&candidate.to_head)
                    || !target_by_candidate
                        .contains_key(&(candidate.link_id.as_str(), candidate.start_ns))
            }))
    {
        return Err("body-frame gimbal configuration must cover all PAT candidates".into());
    }
    if candidates.len() > MAX_CANDIDATES {
        return Err("PAT candidate budget exceeded".into());
    }
    let mut sorted = candidates.to_vec();
    sorted.sort_by(|a, b| {
        (a.start_ns, a.link_id.as_str(), a.end_ns).cmp(&(b.start_ns, b.link_id.as_str(), b.end_ns))
    });

    let mut last_link_end: HashMap<&str, u64> = HashMap::new();
    for candidate in &sorted {
        if candidate.link_id.trim().is_empty()
            || candidate.from_head.trim().is_empty()
            || candidate.to_head.trim().is_empty()
            || candidate.from_head == candidate.to_head
        {
            return Err("PAT link and distinct head IDs must be nonempty".into());
        }
        if candidate.start_ns >= candidate.end_ns || candidate.acquisition_ns == 0 {
            return Err("PAT contact interval and acquisition dwell must be positive".into());
        }
        if candidate
            .slew_ns
            .checked_add(candidate.acquisition_ns)
            .is_none()
        {
            return Err("PAT acquisition duration overflow".into());
        }
        if let Some(previous_end) =
            last_link_end.insert(candidate.link_id.as_str(), candidate.end_ns)
        {
            if candidate.start_ns < previous_end {
                return Err(format!(
                    "overlapping geometric contacts for link {}",
                    candidate.link_id
                ));
            }
        }
    }

    let mut busy_until: HashMap<&str, u64> = HashMap::new();
    let mut windows = Vec::new();
    let mut reservations = Vec::new();
    let mut events = Vec::new();
    for candidate in &sorted {
        events.push(PatEvent {
            link_id: candidate.link_id.clone(),
            at_ns: candidate.start_ns,
            state: PatState::Candidate,
            reason: None,
        });
        let available_ns = candidate
            .start_ns
            .max(*busy_until.get(candidate.from_head.as_str()).unwrap_or(&0))
            .max(*busy_until.get(candidate.to_head.as_str()).unwrap_or(&0));
        let target = target_by_candidate
            .get(&(candidate.link_id.as_str(), candidate.start_ns))
            .copied();
        let slew_ns = if let Some(target) = target {
            let start = start_by_candidate.get(&(candidate.link_id.as_str(), candidate.start_ns));
            let from_direction = start.map(|s| s.from_direction_body).unwrap_or(
                *head_directions
                    .get(&candidate.from_head)
                    .ok_or("missing from-head gimbal state")?,
            );
            let to_direction = start.map(|s| s.to_direction_body).unwrap_or(
                *head_directions
                    .get(&candidate.to_head)
                    .ok_or("missing to-head gimbal state")?,
            );
            let from_ns = slew_and_settle_ns(
                from_direction,
                target.from_target_body,
                head_rates[&candidate.from_head].0,
                head_rates[&candidate.from_head].1,
                head_rates[&candidate.from_head].2.unwrap_or(0),
            )?;
            let to_ns = slew_and_settle_ns(
                to_direction,
                target.to_target_body,
                head_rates[&candidate.to_head].0,
                head_rates[&candidate.to_head].1,
                head_rates[&candidate.to_head].2.unwrap_or(0),
            )?;
            candidate.slew_ns.max(from_ns).max(to_ns)
        } else {
            candidate.slew_ns
        };
        let acquiring_ns = available_ns
            .checked_add(slew_ns)
            .ok_or("PAT slew timestamp overflow")?;
        let tracking_ns = acquiring_ns
            .checked_add(candidate.acquisition_ns)
            .ok_or("PAT acquisition timestamp overflow")?;
        if tracking_ns >= candidate.end_ns {
            events.push(PatEvent {
                link_id: candidate.link_id.clone(),
                at_ns: candidate.start_ns,
                state: PatState::Rejected,
                reason: Some("contact_expires_before_acquisition"),
            });
            continue;
        }
        if slew_ns > 0 {
            events.push(PatEvent {
                link_id: candidate.link_id.clone(),
                at_ns: available_ns,
                state: PatState::Slewing,
                reason: None,
            });
        }
        for (at_ns, state) in [
            (acquiring_ns, PatState::Acquiring),
            (tracking_ns, PatState::Tracking),
            (candidate.end_ns, PatState::Lost),
        ] {
            events.push(PatEvent {
                link_id: candidate.link_id.clone(),
                at_ns,
                state,
                reason: None,
            });
        }
        reservations.push(PatReservation {
            link_id: candidate.link_id.clone(),
            from_head: candidate.from_head.clone(),
            to_head: candidate.to_head.clone(),
            start_ns: available_ns,
            end_ns: candidate.end_ns,
        });
        busy_until.insert(candidate.from_head.as_str(), candidate.end_ns);
        busy_until.insert(candidate.to_head.as_str(), candidate.end_ns);
        if let Some(target) = target {
            head_directions.insert(candidate.from_head.clone(), target.from_target_body);
            head_directions.insert(candidate.to_head.clone(), target.to_target_body);
        }
        windows.push(AcquiredWindow {
            link_id: candidate.link_id.clone(),
            start_ns: tracking_ns,
            end_ns: candidate.end_ns,
        });
    }
    windows.sort_by(|a, b| (a.start_ns, &a.link_id).cmp(&(b.start_ns, &b.link_id)));
    reservations.sort_by(|a, b| (a.start_ns, &a.link_id).cmp(&(b.start_ns, &b.link_id)));
    events.sort_by(|a, b| {
        (a.at_ns, state_order(a.state), &a.link_id).cmp(&(
            b.at_ns,
            state_order(b.state),
            &b.link_id,
        ))
    });
    Ok(PatPlan {
        windows,
        reservations,
        events,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contact(id: &str, from: &str, to: &str, start: u64, end: u64) -> PatCandidate {
        PatCandidate {
            link_id: id.into(),
            from_head: from.into(),
            to_head: to.into(),
            start_ns: start,
            end_ns: end,
            slew_ns: 2,
            acquisition_ns: 3,
        }
    }

    #[test]
    fn lease_transitions_are_deterministic_and_release_before_reacquire() {
        let reservation = |link: &str, from: &str, to: &str, start, end| PatReservation {
            link_id: link.into(),
            from_head: from.into(),
            to_head: to.into(),
            start_ns: start,
            end_ns: end,
        };
        let reservations = vec![
            reservation("z-second", "a", "c", 20, 40),
            reservation("a-first", "a", "b", 0, 20),
            reservation("m-independent", "d", "e", 20, 30),
        ];
        let labels = |input: &[PatReservation]| {
            reservation_transitions(input)
                .unwrap()
                .iter()
                .map(|event| {
                    (
                        event.at_ns,
                        event.kind,
                        input[event.reservation_index].link_id.clone(),
                    )
                })
                .collect::<Vec<_>>()
        };
        use PatLeaseTransitionKind::{Release, Reserve};
        let expected = vec![
            (0, Reserve, "a-first".into()),
            (20, Release, "a-first".into()),
            (20, Reserve, "m-independent".into()),
            (20, Reserve, "z-second".into()),
            (30, Release, "m-independent".into()),
            (40, Release, "z-second".into()),
        ];
        assert_eq!(labels(&reservations), expected);
        let mut reversed = reservations.clone();
        reversed.reverse();
        assert_eq!(labels(&reversed), expected);
    }

    #[test]
    fn lease_transitions_reject_invalid_and_overlapping_reservations() {
        let valid = PatReservation {
            link_id: "first".into(),
            from_head: "a".into(),
            to_head: "b".into(),
            start_ns: 10,
            end_ns: 30,
        };
        let mut conflict = valid.clone();
        conflict.link_id = "second".into();
        conflict.to_head = "c".into();
        conflict.start_ns = 20;
        conflict.end_ns = 40;
        assert!(reservation_transitions(&[valid.clone(), conflict.clone()]).is_err());

        conflict.from_head = "c".into();
        conflict.to_head = "d".into();
        conflict.link_id = "first".into();
        assert!(reservation_transitions(&[valid.clone(), conflict]).is_err());

        let mut invalid = valid.clone();
        invalid.end_ns = invalid.start_ns;
        assert!(reservation_transitions(&[invalid]).is_err());
        let mut invalid = valid.clone();
        invalid.to_head = invalid.from_head.clone();
        assert!(reservation_transitions(&[invalid]).is_err());
        let mut invalid = valid.clone();
        invalid.link_id = " ".into();
        assert!(reservation_transitions(&[invalid]).is_err());

        assert_eq!(reservation_transitions(&[]).unwrap(), vec![]);
        assert_eq!(reservation_transitions(&[valid]).unwrap().len(), 2);
    }

    #[test]
    fn runtime_transitions_release_reserve_then_sample_at_same_instant() {
        let reservations = vec![
            PatReservation {
                link_id: "b".into(),
                from_head: "h1".into(),
                to_head: "h3".into(),
                start_ns: 20,
                end_ns: 40,
            },
            PatReservation {
                link_id: "a".into(),
                from_head: "h1".into(),
                to_head: "h2".into(),
                start_ns: 0,
                end_ns: 20,
            },
        ];
        let samples = [(20, "b"), (5, "a"), (20, "a")];
        let events = pat_runtime_transitions(&reservations, &samples).unwrap();
        use PatRuntimeTransitionKind::{Release, Reserve, Sample};
        assert_eq!(
            events,
            vec![
                PatRuntimeTransition {
                    at_ns: 0,
                    kind: Reserve(1),
                },
                PatRuntimeTransition {
                    at_ns: 5,
                    kind: Sample(1),
                },
                PatRuntimeTransition {
                    at_ns: 20,
                    kind: Release(1),
                },
                PatRuntimeTransition {
                    at_ns: 20,
                    kind: Reserve(0),
                },
                PatRuntimeTransition {
                    at_ns: 20,
                    kind: Sample(2),
                },
                PatRuntimeTransition {
                    at_ns: 20,
                    kind: Sample(0),
                },
                PatRuntimeTransition {
                    at_ns: 40,
                    kind: Release(0),
                },
            ]
        );
        assert_eq!(
            pat_runtime_transitions(&reservations, &[(20, "a"), (20, "a")])
                .unwrap_err(),
            "invalid or duplicate PAT sampled head command"
        );
        assert!(pat_runtime_transitions(&reservations, &[(20, " ")]).is_err());
        let mut overlapping = reservations.clone();
        overlapping[0].start_ns = 19;
        assert!(pat_runtime_transitions(&overlapping, &samples).is_err());
    }

    #[test]
    fn tracking_starts_only_after_slew_and_acquisition() {
        let plan = plan_contacts(&[contact("isl-a-b", "a-head", "b-head", 10, 30)]).unwrap();
        assert_eq!(
            plan.windows,
            vec![AcquiredWindow {
                link_id: "isl-a-b".into(),
                start_ns: 15,
                end_ns: 30
            }]
        );
        assert_eq!(
            plan.reservations,
            vec![PatReservation {
                link_id: "isl-a-b".into(),
                from_head: "a-head".into(),
                to_head: "b-head".into(),
                start_ns: 10,
                end_ns: 30,
            }]
        );
        let timeline: Vec<_> = plan.events.iter().map(|e| (e.at_ns, e.state)).collect();
        assert_eq!(
            timeline,
            vec![
                (10, PatState::Candidate),
                (10, PatState::Slewing),
                (12, PatState::Acquiring),
                (15, PatState::Tracking),
                (30, PatState::Lost)
            ]
        );
    }

    #[test]
    fn too_short_contact_never_establishes_link() {
        let plan = plan_contacts(&[contact("short", "a", "b", 0, 5)]).unwrap();
        assert!(plan.windows.is_empty());
        assert!(plan.reservations.is_empty());
        assert_eq!(plan.events[1].state, PatState::Rejected);
        assert_eq!(
            plan.events[1].reason,
            Some("contact_expires_before_acquisition")
        );
    }

    #[test]
    fn shared_optical_head_waits_for_exclusive_reservation() {
        let plan = plan_contacts(&[
            contact("first", "a", "b", 0, 20),
            contact("second", "a", "c", 10, 40),
        ])
        .unwrap();
        assert_eq!(
            plan.windows
                .iter()
                .map(|w| (w.link_id.as_str(), w.start_ns, w.end_ns))
                .collect::<Vec<_>>(),
            vec![("first", 5, 20), ("second", 25, 40)]
        );
        assert_eq!(
            plan.reservations
                .iter()
                .map(|r| (
                    r.link_id.as_str(),
                    r.from_head.as_str(),
                    r.start_ns,
                    r.end_ns
                ))
                .collect::<Vec<_>>(),
            vec![("first", "a", 0, 20), ("second", "a", 20, 40)]
        );
        assert!(plan
            .events
            .iter()
            .any(|e| { e.link_id == "second" && e.state == PatState::Slewing && e.at_ns == 20 }));
    }

    #[test]
    fn conflict_that_outlasts_contact_is_rejected() {
        let plan = plan_contacts(&[
            contact("first", "a", "b", 0, 20),
            contact("second", "a", "c", 10, 24),
        ])
        .unwrap();
        assert_eq!(plan.windows.len(), 1);
        assert!(plan
            .events
            .iter()
            .any(|e| { e.link_id == "second" && e.state == PatState::Rejected }));
    }

    #[test]
    fn rejected_candidate_does_not_hold_heads_or_delay_next_candidate() {
        let plan = plan_contacts(&[
            contact("a-short", "a", "b", 0, 5),
            contact("z-long", "a", "c", 0, 20),
        ])
        .unwrap();
        assert_eq!(plan.reservations.len(), 1);
        assert_eq!(plan.reservations[0].link_id, "z-long");
        assert_eq!(plan.reservations[0].start_ns, 0);
        assert_eq!(plan.reservations[0].end_ns, 20);
        assert!(plan
            .events
            .iter()
            .any(|e| e.link_id == "a-short" && e.state == PatState::Rejected));
    }

    #[test]
    fn independent_heads_acquire_in_parallel_and_input_order_is_irrelevant() {
        let a = contact("a", "h1", "h2", 0, 20);
        let b = contact("b", "h3", "h4", 0, 20);
        let first = plan_contacts(&[a.clone(), b.clone()]).unwrap();
        let reverse = plan_contacts(&[b, a]).unwrap();
        assert_eq!(first, reverse);
        assert_eq!(first.windows.len(), 2);
        assert!(first.windows.iter().all(|w| w.start_ns == 5));
        assert!(first.reservations.iter().all(|r| r.start_ns == 0));
    }

    #[test]
    fn consecutive_contacts_can_reacquire_at_exclusive_boundary() {
        let plan = plan_contacts(&[
            contact("link", "a", "b", 0, 20),
            contact("link", "a", "b", 20, 40),
        ])
        .unwrap();
        assert_eq!(
            plan.windows.iter().map(|w| w.start_ns).collect::<Vec<_>>(),
            vec![5, 25]
        );
        assert_eq!(
            plan.reservations
                .iter()
                .map(|r| (r.start_ns, r.end_ns))
                .collect::<Vec<_>>(),
            vec![(0, 20), (20, 40)]
        );
        let boundary: Vec<_> = plan.events.iter().filter(|e| e.at_ns == 20).collect();
        assert_eq!(boundary[0].state, PatState::Lost);
        assert_eq!(boundary[1].state, PatState::Candidate);
    }

    #[test]
    fn body_frame_actuator_rate_delays_retargeted_contact() {
        let candidates = [
            contact("link", "a", "b", 0, 120_000_000),
            contact("link", "a", "b", 120_000_000, 200_000_000),
        ];
        let actuators = [
            HeadSlewActuator {
                head_id: "a".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
            HeadSlewActuator {
                head_id: "b".into(),
                initial_direction_body: [-1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
        ];
        let targets = [
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 0,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [-1.0, 0.0, 0.0],
            },
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 120_000_000,
                from_target_body: [0.0, 1.0, 0.0],
                to_target_body: [0.0, -1.0, 0.0],
            },
        ];
        let first = plan_contacts_with_slew(&candidates, &actuators, &targets).unwrap();
        let reversed = plan_contacts_with_slew(
            &[candidates[1].clone(), candidates[0].clone()],
            &actuators,
            &[targets[1].clone(), targets[0].clone()],
        )
        .unwrap();
        assert_eq!(first, reversed);
        assert_eq!(
            first.windows.iter().map(|w| w.start_ns).collect::<Vec<_>>(),
            vec![5, 150_000_003]
        );
        // A very slow actuator cannot acquire within the second contact.
        let slow = actuators.map(|mut a| {
            a.max_slew_rate_deg_per_s = 100.0;
            a
        });
        let slow_plan = plan_contacts_with_slew(&candidates, &slow, &targets).unwrap();
        assert_eq!(slow_plan.windows.len(), 1);
        assert!(slow_plan.events.iter().any(|event| {
            event.at_ns == 120_000_000
                && event.state == PatState::Rejected
                && event.reason == Some("contact_expires_before_acquisition")
        }));
    }

    #[test]
    fn predicted_pat_start_pointing_reuses_follower_state() {
        let candidates = [
            contact("link", "a", "b", 0, 120_000_000),
            contact("link", "a", "b", 120_000_000, 200_000_000),
        ];
        let actuators = ["a", "b"].map(|id| HeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 3000.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let targets = [
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 0,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            },
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 120_000_000,
                from_target_body: [0.0, 1.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            },
        ];
        let starts = [
            PatSlewStart {
                link_id: "link".into(),
                start_ns: 0,
                from_direction_body: [1.0, 0.0, 0.0],
                to_direction_body: [1.0, 0.0, 0.0],
            },
            PatSlewStart {
                link_id: "link".into(),
                start_ns: 120_000_000,
                from_direction_body: [0.0, 1.0, 0.0],
                to_direction_body: [1.0, 0.0, 0.0],
            },
        ];
        let old = plan_contacts_with_slew(&candidates, &actuators, &targets).unwrap();
        let handoff =
            plan_contacts_with_slew_starts(&candidates, &actuators, &targets, &starts).unwrap();
        assert!(handoff.windows[1].start_ns < old.windows[1].start_ns);
        assert_eq!(handoff.windows[1].start_ns, 120_000_005);
        assert!(
            plan_contacts_with_slew_starts(&candidates, &actuators, &targets, &starts[..1])
                .is_err()
        );
        let mut invalid = starts.clone();
        invalid[1].from_direction_body = [0.0; 3];
        assert!(
            plan_contacts_with_slew_starts(&candidates, &actuators, &targets, &invalid).is_err()
        );
    }

    #[test]
    fn body_frame_actuators_reject_missing_targets_and_invalid_vectors() {
        let candidates = [contact("link", "a", "b", 0, 20)];
        let actuators = [
            HeadSlewActuator {
                head_id: "a".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
            HeadSlewActuator {
                head_id: "b".into(),
                initial_direction_body: [-1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
        ];
        let target = PatSlewTarget {
            link_id: "link".into(),
            start_ns: 0,
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [-1.0, 0.0, 0.0],
        };
        assert!(plan_contacts_with_slew(&candidates, &actuators, &[]).is_err());
        assert!(plan_contacts_with_slew(
            &candidates,
            &actuators,
            &[PatSlewTarget {
                from_target_body: [0.0; 3],
                ..target.clone()
            }]
        )
        .is_err());
        assert!(plan_contacts_with_slew(&candidates, &actuators, &[target]).is_ok());
    }

    #[test]
    fn acceleration_limited_gimbal_uses_rest_to_rest_profile() {
        assert_eq!(
            slew_time_ns([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 3000.0, None).unwrap(),
            30_000_000
        );
        assert_eq!(
            slew_time_ns([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 3000.0, Some(200_000.0),).unwrap(),
            45_000_000
        );
        assert_eq!(
            slew_time_ns([1.0, 0.0, 0.0], [1.0, 0.0, 0.0], 3000.0, Some(200_000.0),).unwrap(),
            0
        );
        for invalid in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                slew_time_ns([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 3000.0, Some(invalid),).is_err()
            );
        }

        let actuators = [
            HeadSlewActuator {
                head_id: "a".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200_000.0),
                settling_ns: None,
            },
            HeadSlewActuator {
                head_id: "b".into(),
                initial_direction_body: [-1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200_000.0),
                settling_ns: None,
            },
        ];
        let targets = [PatSlewTarget {
            link_id: "link".into(),
            start_ns: 0,
            from_target_body: [0.0, 1.0, 0.0],
            to_target_body: [0.0, -1.0, 0.0],
        }];
        let candidate = contact("link", "a", "b", 0, 100_000_000);
        let plan = plan_contacts_with_slew(&[candidate], &actuators, &targets).unwrap();
        assert_eq!(plan.windows[0].start_ns, 45_000_003);
        let too_short = contact("link", "a", "b", 0, 45_000_003);
        assert!(plan_contacts_with_slew(&[too_short], &actuators, &targets)
            .unwrap()
            .windows
            .is_empty());
    }

    #[test]
    fn gimbal_settling_delays_only_nonzero_retargets_and_preserves_rejection_state() {
        assert_eq!(
            slew_and_settle_ns(
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                3000.0,
                Some(200_000.0),
                15_000_000,
            )
            .unwrap(),
            0
        );
        assert_eq!(
            slew_and_settle_ns(
                [1.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                3000.0,
                Some(200_000.0),
                15_000_000,
            )
            .unwrap(),
            60_000_000
        );
        assert!(
            slew_and_settle_ns([1.0, 0.0, 0.0], [0.0, 1.0, 0.0], 3000.0, None, u64::MAX,).is_err()
        );

        let candidates = [
            contact("link", "a", "b", 0, 120_000_000),
            contact("link", "a", "b", 120_000_000, 180_000_000),
            contact("link", "a", "b", 180_000_000, 280_000_000),
        ];
        let actuators = [
            HeadSlewActuator {
                head_id: "a".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200_000.0),
                settling_ns: Some(15_000_000),
            },
            HeadSlewActuator {
                head_id: "b".into(),
                initial_direction_body: [-1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200_000.0),
                settling_ns: None,
            },
        ];
        let targets = [
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 0,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [-1.0, 0.0, 0.0],
            },
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 120_000_000,
                from_target_body: [0.0, 1.0, 0.0],
                to_target_body: [0.0, -1.0, 0.0],
            },
            PatSlewTarget {
                link_id: "link".into(),
                start_ns: 180_000_000,
                from_target_body: [0.0, 1.0, 0.0],
                to_target_body: [0.0, -1.0, 0.0],
            },
        ];
        let plan = plan_contacts_with_slew(&candidates, &actuators, &targets).unwrap();
        assert_eq!(
            plan.windows.iter().map(|w| w.start_ns).collect::<Vec<_>>(),
            vec![5, 240_000_003]
        );
        assert!(plan.events.iter().any(|event| {
            event.at_ns == 120_000_000
                && event.state == PatState::Rejected
                && event.reason == Some("contact_expires_before_acquisition")
        }));
        let reversed = plan_contacts_with_slew(
            &[
                candidates[2].clone(),
                candidates[0].clone(),
                candidates[1].clone(),
            ],
            &actuators,
            &[targets[2].clone(), targets[0].clone(), targets[1].clone()],
        )
        .unwrap();
        assert_eq!(plan, reversed);
    }

    #[test]
    fn malformed_contacts_and_overflow_are_rejected() {
        let base = contact("x", "a", "b", 0, 20);
        for invalid in [
            PatCandidate {
                from_head: "b".into(),
                ..base.clone()
            },
            PatCandidate {
                acquisition_ns: 0,
                ..base.clone()
            },
            PatCandidate {
                start_ns: 20,
                ..base.clone()
            },
            PatCandidate {
                slew_ns: u64::MAX,
                ..base.clone()
            },
            PatCandidate {
                link_id: " ".into(),
                ..base.clone()
            },
        ] {
            assert!(plan_contacts(&[invalid]).is_err());
        }
        assert!(plan_contacts(&[
            contact("same", "a", "b", 0, 20),
            contact("same", "a", "b", 10, 30)
        ])
        .is_err());
        assert!(plan_contacts(&[PatCandidate {
            start_ns: u64::MAX - 3,
            end_ns: u64::MAX,
            ..base
        }])
        .is_err());
    }
}
