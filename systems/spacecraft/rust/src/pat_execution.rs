//! SYS-02 typed virtual-time PAT execution boundary.
//!
//! This owner composes the existing head actuator/lease runtime with per-link
//! fault reducers. It is an execution API, not a shared-head contact planner.

use crate::{
    BodyHeadPatRuntimeOutcome, BodyHeadPatRuntimeTransition, BodyHeadRuntime, PatFaultBatchOutcome,
    PatFaultState,
};
use ntarlink_contracts::{FaultCause, LinkAction, OpticalHeadSlewActuator};
use std::collections::{HashMap, HashSet};

/// Equal-time events must enter in this order, independently of link identity.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum PatExecutionPhase {
    Fault,
    Release,
    Reserve,
    Command,
    // A sampled pointing result derived from a command cannot precede it.
    Observation,
}

/// One explicit virtual-time transition. A fault batch is atomic for one link;
/// head transitions retain their existing atomic paired-lease semantics.
#[derive(Debug)]
pub enum PatExecutionTransition<'a> {
    FaultBatch {
        link_id: &'a str,
        at_ns: u64,
        edges: &'a [(FaultCause, LinkAction)],
    },
    /// A sampled-pointing result observed after commands at the same instant.
    /// Unlike a pre-command fault, it cannot invalidate its own observation.
    PointingObservation {
        link_id: &'a str,
        at_ns: u64,
        action: LinkAction,
    },
    Head(BodyHeadPatRuntimeTransition<'a>),
}

#[derive(Debug)]
pub enum PatExecutionOutcome {
    Fault(PatFaultBatchOutcome),
    Head(BodyHeadPatRuntimeOutcome),
}

/// Owns one chronological stream of fault, reservation and command events.
/// The current packet adapter still computes its fault windows offline; this
/// boundary prepares a live event loop without changing planning semantics.
#[derive(Clone, Debug)]
pub struct PatExecutionRuntime {
    heads: BodyHeadRuntime,
    faults: HashMap<String, PatFaultState>,
    /// A link may commit only one atomic fault batch per virtual instant.
    /// The global clock makes the latest timestamp sufficient per link.
    last_fault_batch_at: HashMap<String, u64>,
    last_sampled_fault_at: HashMap<String, u64>,
    last_pointing_observation_at: HashMap<String, u64>,
    /// Last successful command timestamp for each registered link.
    last_successful_command_at: HashMap<String, u64>,
    last_event: Option<(u64, PatExecutionPhase)>,
}

impl PatExecutionRuntime {
    pub fn new(
        actuators: &[OpticalHeadSlewActuator],
        initial_links: &[(&str, bool)],
    ) -> Result<Self, String> {
        let heads = BodyHeadRuntime::new(actuators)?;
        let mut faults = HashMap::new();
        for &(link_id, initially_up) in initial_links {
            if link_id.trim().is_empty()
                || faults
                    .insert(link_id.to_owned(), PatFaultState::new(initially_up))
                    .is_some()
            {
                return Err("invalid or duplicate PAT execution link".into());
            }
        }
        Ok(Self {
            heads,
            faults,
            last_fault_batch_at: HashMap::new(),
            last_sampled_fault_at: HashMap::new(),
            last_pointing_observation_at: HashMap::new(),
            last_successful_command_at: HashMap::new(),
            last_event: None,
        })
    }

    pub fn head_runtime(&self) -> &BodyHeadRuntime {
        &self.heads
    }

    pub fn fault_state(&self, link_id: &str) -> Option<&PatFaultState> {
        self.faults.get(link_id)
    }

    /// The existing offline planner still needs its immutable actuator history.
    pub fn into_heads(self) -> BodyHeadRuntime {
        self.heads
    }

    /// Reject retroactive and phase-reversed events before mutating either
    /// state owner. Failed commands are observations and consume Command phase;
    /// rejected fault batches and lease edges do not advance the global clock.
    pub fn apply(
        &mut self,
        transition: PatExecutionTransition<'_>,
    ) -> Result<PatExecutionOutcome, String> {
        let (at_ns, phase) = match &transition {
            PatExecutionTransition::FaultBatch { at_ns, .. } => (*at_ns, PatExecutionPhase::Fault),
            PatExecutionTransition::PointingObservation { at_ns, .. } => {
                (*at_ns, PatExecutionPhase::Observation)
            }
            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                at_ns, ..
            }) => (*at_ns, PatExecutionPhase::Release),
            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                at_ns, ..
            }) => (*at_ns, PatExecutionPhase::Reserve),
            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(command)) => {
                (command.at_ns, PatExecutionPhase::Command)
            }
        };
        if self
            .last_event
            .is_some_and(|previous| previous > (at_ns, phase))
        {
            return Err("PAT execution transition order mismatch".into());
        }
        let outcome = match transition {
            PatExecutionTransition::FaultBatch {
                link_id,
                at_ns,
                edges,
            } => {
                if edges.is_empty() {
                    return Err("PAT fault batch must contain an edge".into());
                }
                if self.last_fault_batch_at.get(link_id).copied() == Some(at_ns) {
                    return Err("duplicate PAT execution fault batch at timestamp".into());
                }
                let has_sampled = edges
                    .iter()
                    .any(|(cause, _)| *cause == FaultCause::SampledPointing);
                if has_sampled
                    && self.last_pointing_observation_at.get(link_id).copied() == Some(at_ns)
                {
                    return Err("ambiguous PAT sampled-pointing transition at timestamp".into());
                }
                let state = self
                    .faults
                    .get_mut(link_id)
                    .ok_or("unknown PAT execution fault link")?;
                let outcome = state.apply_batch(at_ns, edges.iter().cloned())?;
                self.last_fault_batch_at.insert(link_id.to_owned(), at_ns);
                if has_sampled {
                    self.last_sampled_fault_at.insert(link_id.to_owned(), at_ns);
                }
                PatExecutionOutcome::Fault(outcome)
            }
            PatExecutionTransition::PointingObservation {
                link_id,
                at_ns,
                action,
            } => {
                if self.last_pointing_observation_at.get(link_id).copied() == Some(at_ns) {
                    return Err("duplicate PAT pointing observation at timestamp".into());
                }
                if self.last_sampled_fault_at.get(link_id).copied() == Some(at_ns) {
                    return Err("ambiguous PAT sampled-pointing transition at timestamp".into());
                }
                // A loss must follow a successful command at the same instant.
                // Recovery can come from an external pointing probe: a down
                // link cannot issue successful commands to recover itself.
                if matches!(action, LinkAction::Down)
                    && self.last_successful_command_at.get(link_id).copied() != Some(at_ns)
                {
                    return Err("PAT pointing loss has no successful command at timestamp".into());
                }
                let state = self
                    .faults
                    .get_mut(link_id)
                    .ok_or("unknown PAT execution fault link")?;
                let outcome = state.apply_batch(at_ns, [(FaultCause::SampledPointing, action)])?;
                self.last_pointing_observation_at
                    .insert(link_id.to_owned(), at_ns);
                PatExecutionOutcome::Fault(outcome)
            }
            PatExecutionTransition::Head(transition) => {
                let command_source = match &transition {
                    BodyHeadPatRuntimeTransition::Command(command) => {
                        Some((command.link_id.to_owned(), command.at_ns))
                    }
                    _ => None,
                };
                let head_outcome = match transition {
                    BodyHeadPatRuntimeTransition::Reserve {
                        link_id,
                        from_head,
                        to_head,
                        at_ns,
                    } => {
                        // Head-only replay has no registered fault links.
                        // A registered link must be up to acquire a pair.
                        if !self.faults.is_empty() {
                            let fault = self
                                .faults
                                .get(link_id)
                                .ok_or("unknown PAT execution reservation link")?;
                            if !fault.effective_up() {
                                return Err("PAT execution reservation link is down".into());
                            }
                        }
                        self.heads
                            .apply_pat_transition(BodyHeadPatRuntimeTransition::Reserve {
                                link_id,
                                from_head,
                                to_head,
                                at_ns,
                            })?
                    }
                    BodyHeadPatRuntimeTransition::Command(command) => {
                        // Fault-blocked commands are failed observations, not
                        // fatal lease errors or actuator state mutations.
                        let rejection = if self.faults.is_empty() {
                            None
                        } else {
                            match self.faults.get(command.link_id) {
                                None => Some("unknown PAT execution command link"),
                                Some(state) => match state.down_cause() {
                                    Some(FaultCause::Administrative) => Some(
                                        "PAT execution command blocked by administrative fault",
                                    ),
                                    Some(FaultCause::SampledPointing) => Some(
                                        "PAT execution command blocked by sampled pointing fault",
                                    ),
                                    None => None,
                                },
                            }
                        };
                        match rejection {
                            Some(reason) => self.heads.reject_pat_command(command, reason),
                            None => self.heads.apply_pat_transition(
                                BodyHeadPatRuntimeTransition::Command(command),
                            )?,
                        }
                    }
                    release => self.heads.apply_pat_transition(release)?,
                };
                if matches!(&head_outcome, BodyHeadPatRuntimeOutcome::Command(Ok(_))) {
                    if let Some((link_id, command_at_ns)) = command_source {
                        self.last_successful_command_at
                            .insert(link_id, command_at_ns);
                    }
                }
                PatExecutionOutcome::Head(head_outcome)
            }
        };
        self.last_event = Some((at_ns, phase));
        Ok(outcome)
    }

    /// Execute a mixed batch in deterministic virtual-time and phase order.
    /// The caller may supply events in any order; equal-time events sort by
    /// Fault -> Release -> Reserve -> Command -> Observation, then link and head identity.
    /// Outcomes retain their caller-provided context in execution order.
    /// Same-time, same-phase contention for any head is rejected before the
    /// batch begins, rather than selecting a winner by caller input order.
    /// Multiple fault batches for the same link and instant are also rejected
    /// before any earlier event in the batch can mutate runtime state.
    ///
    /// The batch commits atomically: a later invalid event cannot leave
    /// earlier fault, lease or actuator transitions committed. Failed command
    /// outcomes are observations, not fatal errors, and commit normally.
    /// Staging clones the runtime once per batch (time and memory linear in
    /// existing state size); this does not perform live PAT arbitration.
    pub fn apply_timeline<'a, T>(
        &mut self,
        mut events: Vec<(PatExecutionTransition<'a>, T)>,
    ) -> Result<Vec<(T, PatExecutionOutcome)>, String> {
        // Independent head pairs may act concurrently. A single head cannot
        // have two transitions of the same phase at one virtual instant.
        // Check the entire batch before any fault, lease or command mutation.
        // Release -> Reserve -> Command handoff at one instant remains legal.
        let mut occupied = HashSet::new();
        let mut fault_instants = HashSet::new();
        // A link has one commanded pointing result per instant, even when
        // distinct head pairs would pass the head-contention preflight.
        let mut command_instants = HashSet::new();
        let mut sampled_fault_instants = HashSet::new();
        let mut observation_instants = HashSet::new();
        for (transition, _) in &events {
            let (at_ns, phase, from_head, to_head) = match transition {
                PatExecutionTransition::FaultBatch {
                    link_id,
                    at_ns,
                    edges,
                } => {
                    if !fault_instants.insert((*at_ns, *link_id)) {
                        return Err("duplicate PAT execution fault batch at timestamp".into());
                    }
                    if edges
                        .iter()
                        .any(|(cause, _)| *cause == FaultCause::SampledPointing)
                    {
                        sampled_fault_instants.insert((*at_ns, *link_id));
                    }
                    continue;
                }
                PatExecutionTransition::PointingObservation { link_id, at_ns, .. } => {
                    if !observation_instants.insert((*at_ns, *link_id)) {
                        return Err("duplicate PAT pointing observation at timestamp".into());
                    }
                    continue;
                }
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                    at_ns,
                    from_head,
                    to_head,
                    ..
                }) => (*at_ns, PatExecutionPhase::Release, *from_head, *to_head),
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    at_ns,
                    from_head,
                    to_head,
                    ..
                }) => (*at_ns, PatExecutionPhase::Reserve, *from_head, *to_head),
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(command)) => {
                    if !command_instants.insert((command.at_ns, command.link_id)) {
                        return Err("duplicate PAT link command at timestamp".into());
                    }
                    (
                        command.at_ns,
                        PatExecutionPhase::Command,
                        command.from_head,
                        command.to_head,
                    )
                }
            };
            // Invalid self-pairs retain their existing typed transition error
            // or failed-command observation instead of changing API semantics.
            if from_head == to_head {
                continue;
            }
            if !occupied.insert((at_ns, phase, from_head))
                || !occupied.insert((at_ns, phase, to_head))
            {
                return Err("conflicting PAT head transitions at timestamp".into());
            }
        }

        if observation_instants
            .iter()
            .any(|instant| sampled_fault_instants.contains(instant))
        {
            return Err("ambiguous PAT sampled-pointing transition at timestamp".into());
        }

        fn key<'a>(
            event: &'a PatExecutionTransition<'_>,
        ) -> (u64, PatExecutionPhase, &'a str, &'a str, &'a str) {
            match event {
                PatExecutionTransition::FaultBatch { link_id, at_ns, .. } => {
                    (*at_ns, PatExecutionPhase::Fault, link_id, "", "")
                }
                PatExecutionTransition::PointingObservation { link_id, at_ns, .. } => {
                    (*at_ns, PatExecutionPhase::Observation, link_id, "", "")
                }
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                    link_id,
                    from_head,
                    to_head,
                    at_ns,
                }) => (
                    *at_ns,
                    PatExecutionPhase::Release,
                    link_id,
                    from_head,
                    to_head,
                ),
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id,
                    from_head,
                    to_head,
                    at_ns,
                }) => (
                    *at_ns,
                    PatExecutionPhase::Reserve,
                    link_id,
                    from_head,
                    to_head,
                ),
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(command)) => (
                    command.at_ns,
                    PatExecutionPhase::Command,
                    command.link_id,
                    command.from_head,
                    command.to_head,
                ),
            }
        }

        events.sort_by(|(left, _), (right, _)| key(left).cmp(&key(right)));
        // Execute against a private snapshot so even a late invalid event
        // cannot leak a fault, lease, failed-attempt ledger or head motion.
        let mut staged = self.clone();
        let outcomes = events
            .into_iter()
            .map(|(event, context)| staged.apply(event).map(|outcome| (context, outcome)))
            .collect::<Result<Vec<_>, _>>()?;
        *self = staged;
        Ok(outcomes)
    }

    /// Compatibility bridge for the canonical SYS-03 head-only timeline.
    pub fn apply_head(
        &mut self,
        transition: BodyHeadPatRuntimeTransition<'_>,
    ) -> Result<BodyHeadPatRuntimeOutcome, String> {
        match self.apply(PatExecutionTransition::Head(transition))? {
            PatExecutionOutcome::Head(outcome) => Ok(outcome),
            PatExecutionOutcome::Fault(_) => unreachable!("head transition returned fault outcome"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BodyHeadPatCommand;

    fn heads() -> Vec<OpticalHeadSlewActuator> {
        ["a", "b"]
            .map(|head_id| OpticalHeadSlewActuator {
                head_id: head_id.into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            })
            .to_vec()
    }

    #[test]
    fn command_derived_pointing_fault_follows_its_own_command() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let command = |at_ns| {
            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                BodyHeadPatCommand {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns,
                    leased: false,
                    from_target_body: [1.0, 0.0, 0.0],
                    to_target_body: [1.0, 0.0, 0.0],
                },
            ))
        };
        let outcomes = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::PointingObservation {
                        link_id: "isl",
                        at_ns: 10,
                        action: LinkAction::Down,
                    },
                    "observation",
                ),
                (command(11), "blocked"),
                (command(10), "command"),
            ])
            .unwrap();
        assert_eq!(
            outcomes
                .iter()
                .map(|(context, _)| *context)
                .collect::<Vec<_>>(),
            ["command", "observation", "blocked"]
        );
        assert!(matches!(
            &outcomes[0].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        ));
        assert!(matches!(
            &outcomes[1].1,
            PatExecutionOutcome::Fault(result) if result.was_up && !result.now_up
        ));
        assert!(matches!(
            &outcomes[2].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Err(reason)))
                if reason == "PAT execution command blocked by sampled pointing fault"
        ));
        runtime
            .apply(PatExecutionTransition::PointingObservation {
                link_id: "isl",
                at_ns: 12,
                action: LinkAction::Up,
            })
            .unwrap();
        assert!(runtime.fault_state("isl").unwrap().effective_up());
        assert!(matches!(
            runtime.apply(command(13)).unwrap(),
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        ));
    }

    #[test]
    fn late_unproven_pointing_loss_rolls_back_prior_command_and_fault() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("other", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let command = BodyHeadPatCommand {
            link_id: "isl",
            from_head: "a",
            to_head: "b",
            at_ns: 10,
            leased: false,
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [1.0, 0.0, 0.0],
        };

        // The final observation has no successful command for its own link.
        // Its failure must also roll back an earlier valid command and fault.
        let error = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::PointingObservation {
                        link_id: "other",
                        at_ns: 11,
                        action: LinkAction::Down,
                    },
                    "invalid-observation",
                ),
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        command.clone(),
                    )),
                    "command",
                ),
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "other",
                        at_ns: 9,
                        edges: &down,
                    },
                    "fault",
                ),
            ])
            .unwrap_err();
        assert_eq!(
            error,
            "PAT pointing loss has no successful command at timestamp"
        );
        assert!(runtime.fault_state("other").unwrap().effective_up());

        // The failed batch did not advance the clock or retain actuator
        // history: the same command can be committed in a fresh timeline.
        let outcomes = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        command,
                    )),
                    "command",
                ),
                (
                    PatExecutionTransition::PointingObservation {
                        link_id: "isl",
                        at_ns: 10,
                        action: LinkAction::Down,
                    },
                    "observation",
                ),
            ])
            .unwrap();
        assert_eq!(
            outcomes
                .iter()
                .map(|(context, _)| *context)
                .collect::<Vec<_>>(),
            ["command", "observation"]
        );
        assert!(matches!(
            &outcomes[0].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        ));
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        assert!(runtime.fault_state("other").unwrap().effective_up());
    }

    #[test]
    fn pointing_loss_requires_successful_same_link_command() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("other", true)]).unwrap();
        fn loss(link_id: &str, at_ns: u64) -> PatExecutionTransition<'_> {
            PatExecutionTransition::PointingObservation {
                link_id,
                at_ns,
                action: LinkAction::Down,
            }
        }
        assert_eq!(
            runtime.apply(loss("isl", 10)).unwrap_err(),
            "PAT pointing loss has no successful command at timestamp"
        );
        let command = BodyHeadPatCommand {
            link_id: "isl",
            from_head: "a",
            to_head: "b",
            at_ns: 10,
            leased: false,
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [1.0, 0.0, 0.0],
        };
        assert!(matches!(
            runtime
                .apply(PatExecutionTransition::Head(
                    BodyHeadPatRuntimeTransition::Command(command)
                ))
                .unwrap(),
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        ));
        assert_eq!(
            runtime.apply(loss("other", 10)).unwrap_err(),
            "PAT pointing loss has no successful command at timestamp"
        );
        assert!(runtime.apply(loss("isl", 10)).is_ok());
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        runtime
            .apply(PatExecutionTransition::PointingObservation {
                link_id: "isl",
                at_ns: 11,
                action: LinkAction::Up,
            })
            .unwrap();
        assert!(runtime.fault_state("isl").unwrap().effective_up());
    }

    #[test]
    fn pointing_observation_conflicts_fail_before_any_timeline_mutation() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let admin = [(FaultCause::Administrative, LinkAction::Down)];
        let sampled = [(FaultCause::SampledPointing, LinkAction::Down)];
        assert_eq!(
            runtime
                .apply_timeline(vec![
                    (
                        PatExecutionTransition::PointingObservation {
                            link_id: "isl",
                            at_ns: 10,
                            action: LinkAction::Up,
                        },
                        0,
                    ),
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "isl",
                            at_ns: 10,
                            edges: &sampled,
                        },
                        1,
                    ),
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "isl",
                            at_ns: 9,
                            edges: &admin,
                        },
                        2,
                    ),
                ])
                .unwrap_err(),
            "ambiguous PAT sampled-pointing transition at timestamp"
        );
        assert!(runtime.fault_state("isl").unwrap().effective_up());

        let outcomes = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::PointingObservation {
                        link_id: "isl",
                        at_ns: 10,
                        action: LinkAction::Up,
                    },
                    "observed",
                ),
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "isl",
                        at_ns: 10,
                        edges: &admin,
                    },
                    "admin",
                ),
            ])
            .unwrap();
        assert_eq!(
            outcomes
                .iter()
                .map(|(context, _)| *context)
                .collect::<Vec<_>>(),
            ["admin", "observed"]
        );
        assert_eq!(
            runtime.fault_state("isl").unwrap().down_cause(),
            Some(FaultCause::Administrative)
        );
        let committed = *runtime.fault_state("isl").unwrap();
        assert_eq!(
            runtime
                .apply(PatExecutionTransition::PointingObservation {
                    link_id: "isl",
                    at_ns: 10,
                    action: LinkAction::Up,
                })
                .unwrap_err(),
            "duplicate PAT pointing observation at timestamp"
        );
        assert_eq!(*runtime.fault_state("isl").unwrap(), committed);
        runtime
            .apply(PatExecutionTransition::PointingObservation {
                link_id: "isl",
                at_ns: 11,
                action: LinkAction::Up,
            })
            .unwrap();
        assert_eq!(
            runtime.fault_state("isl").unwrap().down_cause(),
            Some(FaultCause::Administrative)
        );
    }

    #[test]
    fn duplicate_pointing_observations_reject_atomic_batch() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let admin = [(FaultCause::Administrative, LinkAction::Down)];
        assert_eq!(
            runtime
                .apply_timeline(vec![
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "isl",
                            at_ns: 1,
                            edges: &admin,
                        },
                        0,
                    ),
                    (
                        PatExecutionTransition::PointingObservation {
                            link_id: "isl",
                            at_ns: 2,
                            action: LinkAction::Down,
                        },
                        1,
                    ),
                    (
                        PatExecutionTransition::PointingObservation {
                            link_id: "isl",
                            at_ns: 2,
                            action: LinkAction::Up,
                        },
                        2,
                    ),
                ])
                .unwrap_err(),
            "duplicate PAT pointing observation at timestamp"
        );
        assert!(runtime.fault_state("isl").unwrap().effective_up());
    }

    #[test]
    fn mixed_pat_events_share_one_time_and_phase_clock() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let outcome = runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        assert!(
            matches!(outcome, PatExecutionOutcome::Fault(result) if result.was_up && !result.now_up)
        );

        let before = *runtime.fault_state("isl").unwrap();
        let contradictory = [
            (FaultCause::Administrative, LinkAction::Up),
            (FaultCause::Administrative, LinkAction::Down),
        ];
        assert_eq!(
            runtime
                .apply(PatExecutionTransition::FaultBatch {
                    link_id: "isl",
                    at_ns: 20,
                    edges: &contradictory,
                })
                .unwrap_err(),
            "conflicting PAT fault edges at same timestamp"
        );
        assert_eq!(*runtime.fault_state("isl").unwrap(), before);

        let up = [(FaultCause::Administrative, LinkAction::Up)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 20,
                edges: &up,
            })
            .unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 20,
            })
            .unwrap();
        let result = runtime
            .apply_head(BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 20,
                leased: true,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            }))
            .unwrap();
        assert!(matches!(result, BodyHeadPatRuntimeOutcome::Command(Ok(_))));
        assert_eq!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 20,
                })
                .unwrap_err(),
            "PAT execution transition order mismatch"
        );
        assert_eq!(
            runtime
                .apply(PatExecutionTransition::FaultBatch {
                    link_id: "isl",
                    at_ns: 20,
                    edges: &down,
                })
                .unwrap_err(),
            "PAT execution transition order mismatch"
        );
        assert!(runtime.fault_state("isl").unwrap().effective_up());
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Release {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 21,
            })
            .unwrap();
        assert!(runtime.head_runtime().predicted_at("a", 21).is_ok());
    }

    #[test]
    fn same_link_fault_batches_cannot_split_one_atomic_instant() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let up = [(FaultCause::Administrative, LinkAction::Up)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        let committed = *runtime.fault_state("isl").unwrap();

        // A second batch at the same instant would make the final state
        // depend on API call order, even if the individual batches are valid.
        for edges in [&down[..], &up[..]] {
            assert_eq!(
                runtime
                    .apply(PatExecutionTransition::FaultBatch {
                        link_id: "isl",
                        at_ns: 10,
                        edges,
                    })
                    .unwrap_err(),
                "duplicate PAT execution fault batch at timestamp"
            );
            assert_eq!(*runtime.fault_state("isl").unwrap(), committed);
        }

        // A different link can commit its own batch at the same instant.
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "backup",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        assert!(!runtime.fault_state("backup").unwrap().effective_up());

        // A rejected contradictory batch does not consume the next instant.
        let conflicting = [
            (FaultCause::Administrative, LinkAction::Up),
            (FaultCause::Administrative, LinkAction::Down),
        ];
        assert_eq!(
            runtime
                .apply(PatExecutionTransition::FaultBatch {
                    link_id: "isl",
                    at_ns: 11,
                    edges: &conflicting,
                })
                .unwrap_err(),
            "conflicting PAT fault edges at same timestamp"
        );
        assert_eq!(*runtime.fault_state("isl").unwrap(), committed);
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 11,
                edges: &up,
            })
            .unwrap();
        assert!(runtime.fault_state("isl").unwrap().effective_up());
        assert!(!runtime.fault_state("backup").unwrap().effective_up());
    }

    #[test]
    fn rejected_commands_advance_phase_but_invalid_faults_do_not() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", false)]).unwrap();
        assert!(PatExecutionRuntime::new(&heads(), &[("isl", true), ("isl", false)]).is_err());
        let empty: [(FaultCause, LinkAction); 0] = [];
        assert!(runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &empty,
            })
            .is_err());
        assert!(runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "missing",
                at_ns: 10,
                edges: &[(FaultCause::Administrative, LinkAction::Down)],
            })
            .is_err());
        let outcome = runtime
            .apply_head(BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 9,
                leased: false,
                from_target_body: [f64::NAN, 0.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            }))
            .unwrap();
        assert!(matches!(
            outcome,
            BodyHeadPatRuntimeOutcome::Command(Err(_))
        ));
        assert!(runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 9,
                edges: &[(FaultCause::Administrative, LinkAction::Down)],
            })
            .is_err());
        assert!(runtime.fault_state("isl").unwrap().effective_up());

        let down = [(FaultCause::Administrative, LinkAction::Down)];
        for link_id in ["isl", "backup"] {
            runtime
                .apply(PatExecutionTransition::FaultBatch {
                    link_id,
                    at_ns: 10,
                    edges: &down,
                })
                .unwrap();
        }
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        assert!(!runtime.fault_state("backup").unwrap().effective_up());
        assert!(runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 8,
                edges: &down,
            })
            .is_err());
    }
    #[test]
    fn unordered_fault_lease_and_command_events_share_one_timeline() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let up = [(FaultCause::Administrative, LinkAction::Up)];
        let target = [1.0, 0.0, 0.0];
        let events = vec![
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                    BodyHeadPatCommand {
                        link_id: "isl",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 10,
                        leased: true,
                        from_target_body: target,
                        to_target_body: target,
                    },
                )),
                "command",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 10,
                }),
                "reserve",
            ),
            (
                PatExecutionTransition::FaultBatch {
                    link_id: "isl",
                    at_ns: 10,
                    edges: &up,
                },
                "recovery",
            ),
            (
                PatExecutionTransition::FaultBatch {
                    link_id: "isl",
                    at_ns: 5,
                    edges: &down,
                },
                "outage",
            ),
        ];
        let results = runtime.apply_timeline(events).unwrap();
        assert_eq!(
            results
                .iter()
                .map(|(context, _)| *context)
                .collect::<Vec<_>>(),
            vec!["outage", "recovery", "reserve", "command"]
        );
        assert!(matches!(results[0].1, PatExecutionOutcome::Fault(_)));
        assert!(matches!(results[1].1, PatExecutionOutcome::Fault(_)));
        assert!(matches!(
            results[2].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Lease)
        ));
        assert!(matches!(
            results[3].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        ));
        assert!(runtime.fault_state("isl").unwrap().effective_up());
        assert!(runtime.head_runtime().predicted_at("a", 10).is_ok());
    }

    #[test]
    fn timeline_orders_independent_equal_time_faults_by_link_id() {
        let mut runtime = PatExecutionRuntime::new(&[], &[("z", true), ("a", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let events = vec![
            (
                PatExecutionTransition::FaultBatch {
                    link_id: "z",
                    at_ns: 7,
                    edges: &down,
                },
                "z",
            ),
            (
                PatExecutionTransition::FaultBatch {
                    link_id: "a",
                    at_ns: 7,
                    edges: &down,
                },
                "a",
            ),
        ];
        let outcomes = runtime.apply_timeline(events).unwrap();
        assert_eq!(
            outcomes
                .into_iter()
                .map(|(link, _)| link)
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert!(!runtime.fault_state("a").unwrap().effective_up());
        assert!(!runtime.fault_state("z").unwrap().effective_up());
    }
    #[test]
    fn registered_fault_state_gates_reservation_but_not_release() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", true)]).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let up = [(FaultCause::Administrative, LinkAction::Up)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();

        fn reserve(link_id: &str, at_ns: u64) -> PatExecutionTransition<'_> {
            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                link_id,
                from_head: "a",
                to_head: "b",
                at_ns,
            })
        }
        assert_eq!(
            runtime.apply(reserve("isl", 10)).unwrap_err(),
            "PAT execution reservation link is down"
        );
        assert_eq!(
            runtime.apply(reserve("missing", 10)).unwrap_err(),
            "unknown PAT execution reservation link"
        );

        // Rejected reservations must not consume the Reserve phase: another
        // link can still commit a fault batch at this exact instant.
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "backup",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 11,
                edges: &up,
            })
            .unwrap();
        runtime.apply(reserve("isl", 11)).unwrap();
        let command = BodyHeadPatCommand {
            link_id: "isl",
            from_head: "a",
            to_head: "b",
            at_ns: 11,
            leased: true,
            from_target_body: [1.0, 0.0, 0.0],
            to_target_body: [1.0, 0.0, 0.0],
        };
        assert!(matches!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Command(command))
                .unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));

        // A fault at the next instant must not prevent release of the
        // occupied heads. A different, healthy link can then acquire them.
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 12,
                edges: &down,
            })
            .unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Release {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 12,
            })
            .unwrap();
        assert_eq!(
            runtime.apply(reserve("backup", 12)).unwrap_err(),
            "PAT execution reservation link is down"
        );
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "backup",
                at_ns: 13,
                edges: &up,
            })
            .unwrap();
        runtime.apply(reserve("backup", 13)).unwrap();
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        assert!(runtime.fault_state("backup").unwrap().effective_up());
    }

    #[test]
    fn down_link_command_preserves_actuator_and_failure_provenance() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        let target = [1.0, 0.0, 0.0];
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 5,
            })
            .unwrap();
        let command = |at_ns| {
            BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns,
                leased: true,
                from_target_body: target,
                to_target_body: target,
            })
        };
        assert!(matches!(
            runtime.apply_head(command(5)).unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));
        let before = runtime.head_runtime().predicted_at("a", 10).unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        let outcome = runtime.apply_head(command(10)).unwrap();
        let BodyHeadPatRuntimeOutcome::Command(Err(reason)) = outcome else {
            panic!("down-link command must be an observed failure");
        };
        assert_eq!(
            reason,
            "PAT execution command blocked by administrative fault"
        );
        assert_eq!(
            runtime.head_runtime().predicted_at("a", 10).unwrap(),
            before
        );
        runtime
            .head_runtime()
            .verify_sample_command("a", "b", 10, target, target, &Err(reason))
            .unwrap();
        assert_eq!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 10,
                })
                .unwrap_err(),
            "PAT execution transition order mismatch"
        );
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Release {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 11,
            })
            .unwrap();
    }

    #[test]
    fn command_fault_cause_follows_committed_domains_and_recovery() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 1,
            })
            .unwrap();
        let command = |at_ns| {
            BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns,
                leased: true,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            })
        };
        let down = [
            (FaultCause::Administrative, LinkAction::Down),
            (FaultCause::SampledPointing, LinkAction::Down),
        ];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 10,
                edges: &down,
            })
            .unwrap();
        assert!(matches!(
            runtime.apply_head(command(10)).unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Err(reason))
                if reason == "PAT execution command blocked by administrative fault"
        ));
        let admin_up = [(FaultCause::Administrative, LinkAction::Up)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 11,
                edges: &admin_up,
            })
            .unwrap();
        assert!(matches!(
            runtime.apply_head(command(11)).unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Err(reason))
                if reason == "PAT execution command blocked by sampled pointing fault"
        ));
        let pointing_up = [(FaultCause::SampledPointing, LinkAction::Up)];
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 12,
                edges: &pointing_up,
            })
            .unwrap();
        assert!(matches!(
            runtime.apply_head(command(12)).unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));
    }

    #[test]
    fn unknown_registered_command_link_fails_and_allows_same_time_retry() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        fn command(link_id: &str) -> BodyHeadPatRuntimeTransition<'_> {
            BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                link_id,
                from_head: "a",
                to_head: "b",
                at_ns: 10,
                leased: false,
                from_target_body: [1.0, 0.0, 0.0],
                to_target_body: [1.0, 0.0, 0.0],
            })
        }
        let outcome = runtime.apply_head(command("missing")).unwrap();
        let BodyHeadPatRuntimeOutcome::Command(Err(reason)) = outcome else {
            panic!("unknown command link must be an observed failure");
        };
        assert_eq!(reason, "unknown PAT execution command link");
        runtime
            .head_runtime()
            .verify_sample_command("a", "b", 10, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0], &Err(reason))
            .unwrap();
        assert!(matches!(
            runtime.apply_head(command("isl")).unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));
    }

    #[test]
    fn unregistered_head_only_timeline_keeps_offline_compatibility() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[]).unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "offline",
                from_head: "a",
                to_head: "b",
                at_ns: 10,
            })
            .unwrap();
        assert!(matches!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                    link_id: "offline",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 10,
                    leased: true,
                    from_target_body: [1.0, 0.0, 0.0],
                    to_target_body: [1.0, 0.0, 0.0],
                }))
                .unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));
    }
    #[test]
    fn simultaneous_head_contention_rejects_before_any_batch_mutation() {
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        for reverse in [false, true] {
            let mut runtime =
                PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", true)]).unwrap();
            let mut events = vec![
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "isl",
                        at_ns: 4,
                        edges: &down,
                    },
                    "fault",
                ),
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                        link_id: "isl",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 5,
                    }),
                    "first",
                ),
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                        link_id: "backup",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 5,
                    }),
                    "second",
                ),
            ];
            if reverse {
                events.reverse();
            }
            assert_eq!(
                runtime.apply_timeline(events).unwrap_err(),
                "conflicting PAT head transitions at timestamp"
            );
            assert!(runtime.fault_state("isl").unwrap().effective_up());
            // Neither the earlier fault nor the first reservation committed.
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id: "backup",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 5,
                })
                .unwrap();

            let mut releases = vec![
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                        link_id: "backup",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 6,
                    }),
                    "release-1",
                ),
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                        link_id: "backup",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 6,
                    }),
                    "release-2",
                ),
            ];
            if reverse {
                releases.reverse();
            }
            assert_eq!(
                runtime.apply_timeline(releases).unwrap_err(),
                "conflicting PAT head transitions at timestamp"
            );
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "backup",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 6,
                })
                .unwrap();
        }

        for reverse in [false, true] {
            let mut runtime = PatExecutionRuntime::new(&heads(), &[]).unwrap();
            let target = [1.0, 0.0, 0.0];
            let other = [0.0, 1.0, 0.0];
            let mut commands = vec![
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        BodyHeadPatCommand {
                            link_id: "isl",
                            from_head: "a",
                            to_head: "b",
                            at_ns: 10,
                            leased: false,
                            from_target_body: target,
                            to_target_body: target,
                        },
                    )),
                    "first",
                ),
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        BodyHeadPatCommand {
                            link_id: "backup",
                            from_head: "a",
                            to_head: "b",
                            at_ns: 10,
                            leased: false,
                            from_target_body: other,
                            to_target_body: other,
                        },
                    )),
                    "second",
                ),
            ];
            if reverse {
                commands.reverse();
            }
            assert_eq!(
                runtime.apply_timeline(commands).unwrap_err(),
                "conflicting PAT head transitions at timestamp"
            );
            assert!(matches!(
                runtime
                    .apply_head(BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                        link_id: "isl",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 10,
                        leased: false,
                        from_target_body: target,
                        to_target_body: target,
                    }))
                    .unwrap(),
                BodyHeadPatRuntimeOutcome::Command(Ok(_))
            ));
        }
    }

    #[test]
    fn split_same_instant_fault_batches_reject_before_any_batch_mutation() {
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let up = [(FaultCause::Administrative, LinkAction::Up)];
        for second in [&down[..], &up[..]] {
            for reverse in [false, true] {
                let mut runtime =
                    PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", true)]).unwrap();
                let mut events = vec![
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "backup",
                            at_ns: 1,
                            edges: &down,
                        },
                        "earlier",
                    ),
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "isl",
                            at_ns: 3,
                            edges: &down,
                        },
                        "first",
                    ),
                    (
                        PatExecutionTransition::FaultBatch {
                            link_id: "isl",
                            at_ns: 3,
                            edges: second,
                        },
                        "second",
                    ),
                ];
                if reverse {
                    events.reverse();
                }
                assert_eq!(
                    runtime.apply_timeline(events).unwrap_err(),
                    "duplicate PAT execution fault batch at timestamp"
                );
                assert!(runtime.fault_state("isl").unwrap().effective_up());
                assert!(runtime.fault_state("backup").unwrap().effective_up());
                // A failed preflight must not advance the clock or reserve a head.
                runtime
                    .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                        link_id: "isl",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 0,
                    })
                    .unwrap();
                runtime
                    .apply(PatExecutionTransition::FaultBatch {
                        link_id: "backup",
                        at_ns: 1,
                        edges: &down,
                    })
                    .unwrap();
                assert!(!runtime.fault_state("backup").unwrap().effective_up());
            }
        }

        let mut runtime =
            PatExecutionRuntime::new(&[], &[("isl", true), ("backup", true)]).unwrap();
        let outcomes = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "isl",
                        at_ns: 3,
                        edges: &down,
                    },
                    "isl",
                ),
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "backup",
                        at_ns: 3,
                        edges: &down,
                    },
                    "backup",
                ),
            ])
            .unwrap();
        assert_eq!(
            outcomes
                .iter()
                .map(|(context, _)| *context)
                .collect::<Vec<_>>(),
            vec!["backup", "isl"]
        );
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        assert!(!runtime.fault_state("backup").unwrap().effective_up());
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "isl",
                at_ns: 4,
                edges: &up,
            })
            .unwrap();
        assert!(runtime.fault_state("isl").unwrap().effective_up());
    }

    #[test]
    fn independent_heads_and_same_instant_handoff_remain_legal() {
        let actuators = ["a", "b", "c", "d"]
            .map(|head_id| OpticalHeadSlewActuator {
                head_id: head_id.into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            })
            .to_vec();
        let mut runtime = PatExecutionRuntime::new(
            &actuators,
            &[("old", true), ("new", true), ("independent", true)],
        )
        .unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "old",
                from_head: "a",
                to_head: "b",
                at_ns: 1,
            })
            .unwrap();
        let target = [1.0, 0.0, 0.0];
        let events = vec![
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                    BodyHeadPatCommand {
                        link_id: "independent",
                        from_head: "c",
                        to_head: "d",
                        at_ns: 10,
                        leased: true,
                        from_target_body: target,
                        to_target_body: target,
                    },
                )),
                "independent-command",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id: "new",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 10,
                }),
                "new-reserve",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "old",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 10,
                }),
                "old-release",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id: "independent",
                    from_head: "c",
                    to_head: "d",
                    at_ns: 10,
                }),
                "independent-reserve",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                    BodyHeadPatCommand {
                        link_id: "new",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 10,
                        leased: true,
                        from_target_body: target,
                        to_target_body: target,
                    },
                )),
                "new-command",
            ),
        ];
        let results = runtime.apply_timeline(events).unwrap();
        assert_eq!(results.len(), 5);
        assert!(results.iter().all(|(_, outcome)| {
            matches!(
                outcome,
                PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Lease)
                    | PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
            )
        }));
        assert!(runtime.head_runtime().predicted_at("a", 10).is_ok());
        assert!(runtime.head_runtime().predicted_at("d", 10).is_ok());
    }

    #[test]
    fn late_mixed_timeline_error_rolls_back_fault_lease_and_actuator_state() {
        let mut runtime =
            PatExecutionRuntime::new(&heads(), &[("isl", true), ("backup", true)]).unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 1,
            })
            .unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let target = [0.0, 1.0, 0.0];
        let events = vec![
            (
                PatExecutionTransition::FaultBatch {
                    link_id: "backup",
                    at_ns: 2,
                    edges: &down,
                },
                "fault",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                    BodyHeadPatCommand {
                        link_id: "isl",
                        from_head: "a",
                        to_head: "b",
                        at_ns: 2,
                        leased: true,
                        from_target_body: target,
                        to_target_body: target,
                    },
                )),
                "command",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 3,
                }),
                "release",
            ),
            (
                PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Reserve {
                    link_id: "backup",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 3,
                }),
                "invalid-reserve",
            ),
        ];
        assert_eq!(
            runtime.apply_timeline(events).unwrap_err(),
            "PAT execution reservation link is down"
        );
        assert!(runtime.fault_state("backup").unwrap().effective_up());
        let history = runtime.head_runtime().predicted_at("a", 1_000_000_000);
        assert_eq!(
            history.unwrap_err(),
            "missing predicted optical head history"
        );
        // The original lease and global clock remain at the pre-batch state.
        runtime
            .apply(PatExecutionTransition::FaultBatch {
                link_id: "backup",
                at_ns: 2,
                edges: &down,
            })
            .unwrap();
        assert!(matches!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Command(BodyHeadPatCommand {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 2,
                    leased: true,
                    from_target_body: [1.0, 0.0, 0.0],
                    to_target_body: [1.0, 0.0, 0.0],
                }))
                .unwrap(),
            BodyHeadPatRuntimeOutcome::Command(Ok(_))
        ));
    }

    #[test]
    fn failed_command_observation_commits_with_successful_atomic_batch() {
        let mut runtime = PatExecutionRuntime::new(&heads(), &[("isl", true)]).unwrap();
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Reserve {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 1,
            })
            .unwrap();
        let down = [(FaultCause::Administrative, LinkAction::Down)];
        let target = [1.0, 0.0, 0.0];
        let results = runtime
            .apply_timeline(vec![
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        BodyHeadPatCommand {
                            link_id: "isl",
                            from_head: "a",
                            to_head: "b",
                            at_ns: 2,
                            leased: true,
                            from_target_body: target,
                            to_target_body: target,
                        },
                    )),
                    "command",
                ),
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "isl",
                        at_ns: 2,
                        edges: &down,
                    },
                    "fault",
                ),
            ])
            .unwrap();
        assert_eq!(results.len(), 2);
        assert!(matches!(
            &results[1].1,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Err(reason)))
                if reason == "PAT execution command blocked by administrative fault"
        ));
        assert!(!runtime.fault_state("isl").unwrap().effective_up());
        assert_eq!(
            runtime
                .apply_head(BodyHeadPatRuntimeTransition::Release {
                    link_id: "isl",
                    from_head: "a",
                    to_head: "b",
                    at_ns: 2,
                })
                .unwrap_err(),
            "PAT execution transition order mismatch"
        );
        runtime
            .apply_head(BodyHeadPatRuntimeTransition::Release {
                link_id: "isl",
                from_head: "a",
                to_head: "b",
                at_ns: 3,
            })
            .unwrap();
    }

    #[test]
    fn duplicate_link_commands_on_disjoint_heads_fail_before_fault_commit() {
        let heads: Vec<_> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|head_id| OpticalHeadSlewActuator {
                head_id: head_id.into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            })
            .collect();
        let target = [1.0, 0.0, 0.0];
        let down = [(FaultCause::Administrative, LinkAction::Down)];

        for reverse in [false, true] {
            let mut runtime =
                PatExecutionRuntime::new(&heads, &[("isl", true), ("other", true)]).unwrap();
            let mut commands = vec![("a", "b", "first"), ("c", "d", "second")];
            if reverse {
                commands.reverse();
            }
            // An earlier successful command must also roll back if a later
            // duplicate link command fails the batch preflight.
            let earlier_command = || BodyHeadPatCommand {
                link_id: "other",
                from_head: "c",
                to_head: "d",
                at_ns: 3,
                leased: false,
                from_target_body: target,
                to_target_body: target,
            };
            let mut events = vec![
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        earlier_command(),
                    )),
                    "earlier-command",
                ),
                (
                    PatExecutionTransition::FaultBatch {
                        link_id: "other",
                        at_ns: 4,
                        edges: &down,
                    },
                    "fault",
                ),
            ];
            events.extend(commands.into_iter().map(|(from_head, to_head, label)| {
                (
                    PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                        BodyHeadPatCommand {
                            link_id: "isl",
                            from_head,
                            to_head,
                            at_ns: 5,
                            leased: false,
                            from_target_body: target,
                            to_target_body: target,
                        },
                    )),
                    label,
                )
            }));
            assert_eq!(
                runtime.apply_timeline(events).unwrap_err(),
                "duplicate PAT link command at timestamp"
            );
            assert!(runtime.fault_state("other").unwrap().effective_up());
            assert!(runtime.fault_state("isl").unwrap().effective_up());
            assert!(matches!(
                runtime
                    .apply(PatExecutionTransition::Head(
                        BodyHeadPatRuntimeTransition::Command(earlier_command())
                    ))
                    .unwrap(),
                PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
            ));
            assert!(runtime
                .apply(PatExecutionTransition::FaultBatch {
                    link_id: "other",
                    at_ns: 4,
                    edges: &down,
                })
                .is_ok());
        }
    }

    #[test]
    fn simultaneous_commands_on_distinct_links_and_heads_remain_valid() {
        let heads: Vec<_> = ["a", "b", "c", "d"]
            .into_iter()
            .map(|head_id| OpticalHeadSlewActuator {
                head_id: head_id.into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            })
            .collect();
        let mut runtime =
            PatExecutionRuntime::new(&heads, &[("isl", true), ("other", true)]).unwrap();
        let target = [1.0, 0.0, 0.0];
        let outcomes = runtime
            .apply_timeline(
                [("other", "c", "d"), ("isl", "a", "b")]
                    .into_iter()
                    .map(|(link_id, from_head, to_head)| {
                        (
                            PatExecutionTransition::Head(BodyHeadPatRuntimeTransition::Command(
                                BodyHeadPatCommand {
                                    link_id,
                                    from_head,
                                    to_head,
                                    at_ns: 5,
                                    leased: false,
                                    from_target_body: target,
                                    to_target_body: target,
                                },
                            )),
                            link_id,
                        )
                    })
                    .collect(),
            )
            .unwrap();
        assert_eq!(outcomes.len(), 2);
        assert!(outcomes.iter().all(|(_, outcome)| matches!(
            outcome,
            PatExecutionOutcome::Head(BodyHeadPatRuntimeOutcome::Command(Ok(_)))
        )));
        assert_eq!(outcomes[0].0, "isl");
        assert_eq!(outcomes[1].0, "other");
    }
}
