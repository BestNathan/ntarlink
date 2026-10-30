//! SYS-03 deterministic optical terminal acquisition scheduling.
//!
//! This is an offline virtual-time planning primitive. Geometric candidates
//! come from SYS-01; neither a contact nor this schedule is an optical PHY
//! measurement or a packet-runtime admission decision.

use std::collections::HashMap;

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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatPlan {
    pub windows: Vec<AcquiredWindow>,
    pub events: Vec<PatEvent>,
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
        let acquiring_ns = available_ns
            .checked_add(candidate.slew_ns)
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
        if candidate.slew_ns > 0 {
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
        busy_until.insert(candidate.from_head.as_str(), candidate.end_ns);
        busy_until.insert(candidate.to_head.as_str(), candidate.end_ns);
        windows.push(AcquiredWindow {
            link_id: candidate.link_id.clone(),
            start_ns: tracking_ns,
            end_ns: candidate.end_ns,
        });
    }
    windows.sort_by(|a, b| (a.start_ns, &a.link_id).cmp(&(b.start_ns, &b.link_id)));
    events.sort_by(|a, b| {
        (a.at_ns, state_order(a.state), &a.link_id).cmp(&(
            b.at_ns,
            state_order(b.state),
            &b.link_id,
        ))
    });
    Ok(PatPlan { windows, events })
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
    fn independent_heads_acquire_in_parallel_and_input_order_is_irrelevant() {
        let a = contact("a", "h1", "h2", 0, 20);
        let b = contact("b", "h3", "h4", 0, 20);
        let first = plan_contacts(&[a.clone(), b.clone()]).unwrap();
        let reverse = plan_contacts(&[b, a]).unwrap();
        assert_eq!(first, reverse);
        assert_eq!(first.windows.len(), 2);
        assert!(first.windows.iter().all(|w| w.start_ns == 5));
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
        let boundary: Vec<_> = plan.events.iter().filter(|e| e.at_ns == 20).collect();
        assert_eq!(boundary[0].state, PatState::Lost);
        assert_eq!(boundary[1].state, PatState::Candidate);
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
