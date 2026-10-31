use ntarlink_access_scheduler::{BeamConfig, BeamScheduler, TerminalSpec};
use ntarlink_contracts::{
    ContactMode, Fault, FaultCause, LinkAction, LinkKind, OpticalBodyTrackingSample,
    PropagationDelaySample, Scenario,
};
use ntarlink_network_control::next_hops;
use ntarlink_optical_pat::{
    plan_contacts, plan_contacts_with_slew, plan_contacts_with_slew_starts,
    pat_runtime_transitions, HeadSlewActuator, PatCandidate, PatReservation,
    PatRuntimeTransitionKind, PatSlewStart, PatSlewTarget, PatState,
};
use ntarlink_simulation::Scheduler;
use ntarlink_spacecraft::{
    body_tracking_error_allowed, body_tracking_target_rate_allowed,
    geometry_los_acceleration_allowed, geometry_los_rate_allowed, geometry_pointing_allowed,
    pointing_allowed, BodyHeadRuntime, HeadAvailability,
};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

const MAX_PROCESSED_EVENTS: u64 = 5_000_000;

type PredictedPair = ([f64; 3], [f64; 3]);
type PredictedObservations = HashMap<(String, u64), Result<PredictedPair, String>>;

/// Piecewise-linear delay at the completion of packet serialization.
/// Integer arithmetic and nearest-nanosecond rounding preserve reproducibility.
fn interpolated_delay(samples: &[PropagationDelaySample], at_ns: u64) -> u64 {
    let upper = samples.partition_point(|sample| sample.at_ns <= at_ns);
    if upper == 0 {
        return samples[0].delay_ns;
    }
    if upper == samples.len() {
        return samples[upper - 1].delay_ns;
    }
    let a = &samples[upper - 1];
    let b = &samples[upper];
    let span = u128::from(b.at_ns - a.at_ns);
    let elapsed = u128::from(at_ns - a.at_ns);
    let scaled = u128::from(a.delay_ns.abs_diff(b.delay_ns)) * elapsed;
    let rounded = scaled / span + u128::from(scaled % span >= span.div_ceil(2));
    let change = u64::try_from(rounded).expect("interpolated delta is bounded by endpoint delta");
    if b.delay_ns >= a.delay_ns {
        a.delay_ns + change
    } else {
        a.delay_ns - change
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PacketStatus {
    Delivered,
    Dropped,
    InFlight,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HopTrace {
    pub link_id: String,
    pub from: String,
    pub to: String,
    pub enqueued_ns: u64,
    pub transmit_started_ns: u64,
    pub transmit_completed_ns: u64,
    pub arrived_ns: u64,
    pub queue_delay_ns: u64,
    pub serialization_ns: u64,
    pub propagation_ns: u64,
    pub processing_ns: u64,
    pub route_revision: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PacketTrace {
    pub packet_id: String,
    pub flow_id: String,
    pub injected_ns: u64,
    pub completed_ns: Option<u64>,
    pub status: PacketStatus,
    pub drop_reason: Option<String>,
    pub hops: Vec<HopTrace>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ControlEvent {
    pub at_ns: u64,
    pub kind: String,
    pub link_id: Option<String>,
    pub route_revision: u64,
}

/// An actual shared uplink MAC grant issued by SYS-05 in virtual time.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct BeamGrantTrace {
    pub beam_id: String,
    pub frame_start_ns: u64,
    pub terminal_id: String,
    pub link_id: String,
    pub packet_id: String,
    pub wire_bytes: u64,
    pub transmit_started_ns: u64,
    pub transmit_completed_ns: u64,
}

/// Planned SYS-03 PAT state transition; not a measured optical PHY event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PatEventTrace {
    pub link_id: String,
    pub at_ns: u64,
    pub state: &'static str,
    pub reason: Option<&'static str>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SimulationReport {
    pub scenario: String,
    pub fidelity: String,
    pub duration_ns: u64,
    pub processed_events: u64,
    pub route_revision: u64,
    pub injected: usize,
    pub delivered: usize,
    pub dropped: usize,
    pub packets: Vec<PacketTrace>,
    pub control_events: Vec<ControlEvent>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub beam_grants: Vec<BeamGrantTrace>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub pat_events: Vec<PatEventTrace>,
}

enum Event {
    Inject { flow: usize, ordinal: u32 },
    Arrive { packet: usize, node: String },
    Fault { index: usize },
    ContactOpen { index: usize },
    ContactClose { index: usize },
    InstallRoutes,
    BeamFrame { beam: usize },
}

struct PacketRecord {
    flow: usize,
    trace: PacketTrace,
}

#[derive(Default)]
struct LinkRuntime {
    admin_up: bool,
    contact_up: bool,
    contact_delay_ns: Option<u64>,
    active_contact: Option<usize>,
    transmitter_free_ns: u64,
    bytes_in_queue: u64,
    pending_tx: VecDeque<(u64, u64)>,
}

impl LinkRuntime {
    fn effective_up(&self) -> bool {
        self.admin_up && self.contact_up
    }
}

/// Pending packets are owned by SYS-05 until a frame issues a grant.
struct BeamRuntime {
    scheduler: BeamScheduler,
    pending: HashMap<usize, (u64, usize)>,
    next_frame_at: Option<u64>,
}

struct Runtime<'a> {
    scenario: &'a Scenario,
    links: Vec<LinkRuntime>,
    beams: Vec<BeamRuntime>,
    beam_for_link: Vec<Option<usize>>,
    routes: Vec<HashMap<String, usize>>,
    route_revision: u64,
    packets: Vec<PacketRecord>,
    control_events: Vec<ControlEvent>,
    beam_grants: Vec<BeamGrantTrace>,
}

impl<'a> Runtime<'a> {
    fn new(scenario: &'a Scenario) -> Self {
        let links = scenario
            .links
            .iter()
            .map(|link| {
                let contact_defined = scenario.contacts.iter().any(|c| c.link_id == link.id);
                let schedule_required = scenario.contact_mode == ContactMode::Scheduled
                    && matches!(
                        link.kind,
                        LinkKind::UserRf | LinkKind::Feeder | LinkKind::OpticalIsl
                    );
                let initial = scenario
                    .contacts
                    .iter()
                    .enumerate()
                    .find(|(_, c)| c.link_id == link.id && c.start_ns == 0);
                LinkRuntime {
                    admin_up: link.up,
                    contact_up: (!contact_defined && !schedule_required) || initial.is_some(),
                    contact_delay_ns: initial.and_then(|(_, c)| c.propagation_delay_ns),
                    active_contact: initial.map(|(index, _)| index),
                    ..LinkRuntime::default()
                }
            })
            .collect();
        let mut beam_for_link = vec![None; scenario.links.len()];
        let beams = scenario
            .access_beams
            .iter()
            .enumerate()
            .map(|(index, beam)| {
                for bearer in &beam.bearers {
                    let link_index = scenario
                        .links
                        .iter()
                        .position(|l| l.id == bearer.link_id)
                        .expect("validated access bearer link");
                    beam_for_link[link_index] = Some(index);
                }
                let config = BeamConfig {
                    capacity_bps: beam.capacity_bps,
                    frame_duration_ns: beam.frame_duration_ns,
                    quantum_bytes: beam.quantum_bytes,
                    packet_overhead_bytes: beam.packet_overhead_bytes,
                    queue_limit_bytes: beam.queue_limit_bytes,
                };
                let terminals = beam
                    .bearers
                    .iter()
                    .map(|b| TerminalSpec {
                        id: b.terminal_id.clone(),
                        weight: b.weight,
                    })
                    .collect();
                BeamRuntime {
                    scheduler: BeamScheduler::new(config, terminals)
                        .expect("validated access beam configuration"),
                    pending: HashMap::new(),
                    next_frame_at: None,
                }
            })
            .collect();
        let mut runtime = Self {
            scenario,
            links,
            beams,
            beam_for_link,
            routes: Vec::new(),
            route_revision: 0,
            packets: Vec::new(),
            control_events: Vec::new(),
            beam_grants: Vec::new(),
        };
        runtime.install_routes(0);
        runtime
    }

    fn install_routes(&mut self, at_ns: u64) {
        let available: Vec<bool> = self.links.iter().map(LinkRuntime::effective_up).collect();
        let mut effective_links = self.scenario.links.clone();
        for (link, runtime) in effective_links.iter_mut().zip(&self.links) {
            if let Some(delay_ns) = runtime.contact_delay_ns {
                link.propagation_delay_ns = delay_ns;
            }
        }
        self.routes = self
            .scenario
            .flows
            .iter()
            .map(|flow| {
                next_hops(
                    &effective_links,
                    &available,
                    &flow.destination,
                    flow.packet_size_bytes,
                )
            })
            .collect();
        self.route_revision += 1;
        self.control_events.push(ControlEvent {
            at_ns,
            kind: "routes_installed".into(),
            link_id: None,
            route_revision: self.route_revision,
        });
    }

    fn inject(
        &mut self,
        at_ns: u64,
        flow_index: usize,
        ordinal: u32,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let flow = &self.scenario.flows[flow_index];
        let packet_index = self.packets.len();
        self.packets.push(PacketRecord {
            flow: flow_index,
            trace: PacketTrace {
                packet_id: format!("{}:{}", flow.id, ordinal),
                flow_id: flow.id.clone(),
                injected_ns: at_ns,
                completed_ns: None,
                status: PacketStatus::InFlight,
                drop_reason: None,
                hops: Vec::new(),
            },
        });
        events.schedule(
            at_ns,
            20,
            Event::Arrive {
                packet: packet_index,
                node: flow.source.clone(),
            },
        )
    }

    fn drop_packet(&mut self, index: usize, at_ns: u64, reason: &str) {
        let packet = &mut self.packets[index].trace;
        packet.status = PacketStatus::Dropped;
        packet.drop_reason = Some(reason.to_owned());
        packet.completed_ns = Some(at_ns);
    }

    fn arrive(
        &mut self,
        at_ns: u64,
        packet_index: usize,
        node: String,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let flow_index = self.packets[packet_index].flow;
        let flow = &self.scenario.flows[flow_index];
        if node == flow.destination {
            self.packets[packet_index].trace.status = PacketStatus::Delivered;
            self.packets[packet_index].trace.completed_ns = Some(at_ns);
            return Ok(());
        }
        let Some(&link_index) = self.routes[flow_index].get(&node) else {
            self.drop_packet(packet_index, at_ns, "no_route");
            return Ok(());
        };
        if !self.links[link_index].effective_up() {
            self.drop_packet(packet_index, at_ns, "link_unavailable");
            return Ok(());
        }
        if let Some(beam_index) = self.beam_for_link[link_index] {
            return self.enqueue_beam(beam_index, link_index, packet_index, at_ns, events);
        }
        let link = &self.scenario.links[link_index];
        let state = &mut self.links[link_index];
        while state
            .pending_tx
            .front()
            .is_some_and(|(completed, _)| *completed <= at_ns)
        {
            let (_, bytes) = state.pending_tx.pop_front().expect("front exists");
            state.bytes_in_queue -= bytes;
        }
        let size = flow.packet_size_bytes;
        if size > link.queue_limit_bytes.saturating_sub(state.bytes_in_queue) {
            self.drop_packet(packet_index, at_ns, "queue_overflow");
            return Ok(());
        }
        let start_ns = at_ns.max(state.transmitter_free_ns);
        let bits_ns = u128::from(size) * 8 * 1_000_000_000;
        let serialization_ns = u64::try_from(bits_ns.div_ceil(u128::from(link.capacity_bps)))
            .map_err(|_| "serialization time exceeds u64".to_owned())?;
        let completed_ns = start_ns
            .checked_add(serialization_ns)
            .ok_or("transmission time overflow")?;
        let propagation_ns = state
            .active_contact
            .and_then(|index| {
                let samples = &self.scenario.contacts[index].propagation_delay_samples;
                (!samples.is_empty()).then(|| interpolated_delay(samples, completed_ns))
            })
            .unwrap_or_else(|| state.contact_delay_ns.unwrap_or(link.propagation_delay_ns));
        let arrived_ns = completed_ns
            .checked_add(propagation_ns)
            .and_then(|t| t.checked_add(link.processing_delay_ns))
            .ok_or("packet arrival time overflow")?;
        if arrived_ns >= self.scenario.duration_ns {
            self.drop_packet(packet_index, at_ns, "simulation_horizon");
            return Ok(());
        }
        state.pending_tx.push_back((completed_ns, size));
        state.bytes_in_queue += size;
        state.transmitter_free_ns = completed_ns;
        self.packets[packet_index].trace.hops.push(HopTrace {
            link_id: link.id.clone(),
            from: link.from.clone(),
            to: link.to.clone(),
            enqueued_ns: at_ns,
            transmit_started_ns: start_ns,
            transmit_completed_ns: completed_ns,
            arrived_ns,
            queue_delay_ns: start_ns - at_ns,
            serialization_ns,
            propagation_ns,
            processing_ns: link.processing_delay_ns,
            route_revision: self.route_revision,
        });
        // P0 completion policy: transmissions already accepted finish after link-down.
        events.schedule(
            arrived_ns,
            20,
            Event::Arrive {
                packet: packet_index,
                node: link.to.clone(),
            },
        )
    }

    /// Schedule only when there is a pending packet; idle beams do not tick.
    /// Packet arrivals (priority 20) precede frame allocation (priority 30).
    fn ensure_beam_frame(
        &mut self,
        beam_index: usize,
        at_ns: u64,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let beam = &mut self.beams[beam_index];
        if beam.next_frame_at.is_some() || beam.pending.is_empty() {
            return Ok(());
        }
        let duration = self.scenario.access_beams[beam_index].frame_duration_ns;
        let next = u128::from(at_ns).div_ceil(u128::from(duration)) * u128::from(duration);
        if next >= u128::from(self.scenario.duration_ns) {
            return Ok(());
        }
        let next = u64::try_from(next).map_err(|_| "beam frame time overflow")?;
        events.schedule(next, 30, Event::BeamFrame { beam: beam_index })?;
        beam.next_frame_at = Some(next);
        Ok(())
    }

    fn enqueue_beam(
        &mut self,
        beam_index: usize,
        link_index: usize,
        packet_index: usize,
        at_ns: u64,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let link = &self.scenario.links[link_index];
        let flow = &self.scenario.flows[self.packets[packet_index].flow];
        match self.beams[beam_index].scheduler.enqueue(
            &link.from,
            packet_index.to_string(),
            flow.packet_size_bytes,
        ) {
            Ok(()) => {
                self.beams[beam_index]
                    .pending
                    .insert(packet_index, (at_ns, link_index));
                self.ensure_beam_frame(beam_index, at_ns, events)
            }
            Err(reason) => {
                let kind = if reason.contains("queue is full") {
                    "beam_queue_overflow"
                } else {
                    "beam_packet_unserviceable"
                };
                self.drop_packet(packet_index, at_ns, kind);
                Ok(())
            }
        }
    }

    fn discard_beam_link(&mut self, link_index: usize, at_ns: u64) -> Result<(), String> {
        let Some(beam_index) = self.beam_for_link[link_index] else {
            return Ok(());
        };
        let terminal_id = &self.scenario.links[link_index].from;
        let removed = self.beams[beam_index]
            .scheduler
            .discard_terminal(terminal_id)?;
        for id in removed {
            let packet_index: usize = id.parse().map_err(|_| "invalid grant packet ID")?;
            self.beams[beam_index]
                .pending
                .remove(&packet_index)
                .ok_or("missing queued beam packet")?;
            self.drop_packet(packet_index, at_ns, "beam_link_lost");
        }
        Ok(())
    }

    /// Grant packets in a shared frame, with sequential wire-byte serialization.
    /// Issued grants finish under the existing completion-after-departure policy.
    fn beam_frame(
        &mut self,
        at_ns: u64,
        beam_index: usize,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        self.beams[beam_index].next_frame_at = None;
        if self.beams[beam_index].pending.is_empty() {
            return Ok(());
        }
        let report = self.beams[beam_index].scheduler.schedule_frame()?;
        let capacity_bps = self.scenario.access_beams[beam_index].capacity_bps;
        let frame_duration_ns = self.scenario.access_beams[beam_index].frame_duration_ns;
        let mut used_bytes = 0u64;
        for grant in report.grants {
            let packet_index: usize = grant
                .packet_id
                .parse()
                .map_err(|_| "invalid grant packet ID")?;
            let (enqueued_ns, link_index) = self.beams[beam_index]
                .pending
                .remove(&packet_index)
                .ok_or("missing granted packet")?;
            if !self.links[link_index].effective_up() {
                self.drop_packet(packet_index, at_ns, "beam_link_lost");
                continue;
            }
            let start_offset = u64::try_from(
                (u128::from(used_bytes) * 8_000_000_000u128).div_ceil(u128::from(capacity_bps)),
            )
            .map_err(|_| "beam start offset overflow")?;
            used_bytes += grant.wire_bytes;
            let end_offset = u64::try_from(
                (u128::from(used_bytes) * 8_000_000_000u128).div_ceil(u128::from(capacity_bps)),
            )
            .map_err(|_| "beam end offset overflow")?;
            let start_ns = at_ns
                .checked_add(start_offset)
                .ok_or("beam start time overflow")?;
            let completed_ns = at_ns
                .checked_add(end_offset)
                .ok_or("beam completion time overflow")?;
            let link = &self.scenario.links[link_index];
            let state = &self.links[link_index];
            let propagation_ns = state
                .active_contact
                .and_then(|index| {
                    let samples = &self.scenario.contacts[index].propagation_delay_samples;
                    (!samples.is_empty()).then(|| interpolated_delay(samples, completed_ns))
                })
                .unwrap_or_else(|| state.contact_delay_ns.unwrap_or(link.propagation_delay_ns));
            let arrived_ns = completed_ns
                .checked_add(propagation_ns)
                .and_then(|t| t.checked_add(link.processing_delay_ns))
                .ok_or("beam packet arrival time overflow")?;
            self.beam_grants.push(BeamGrantTrace {
                beam_id: self.scenario.access_beams[beam_index].id.clone(),
                frame_start_ns: at_ns,
                terminal_id: grant.terminal_id,
                link_id: link.id.clone(),
                packet_id: self.packets[packet_index].trace.packet_id.clone(),
                wire_bytes: grant.wire_bytes,
                transmit_started_ns: start_ns,
                transmit_completed_ns: completed_ns,
            });
            if arrived_ns >= self.scenario.duration_ns {
                self.drop_packet(packet_index, at_ns, "simulation_horizon");
                continue;
            }
            self.packets[packet_index].trace.hops.push(HopTrace {
                link_id: link.id.clone(),
                from: link.from.clone(),
                to: link.to.clone(),
                enqueued_ns,
                transmit_started_ns: start_ns,
                transmit_completed_ns: completed_ns,
                arrived_ns,
                queue_delay_ns: start_ns - enqueued_ns,
                serialization_ns: completed_ns - start_ns,
                propagation_ns,
                processing_ns: link.processing_delay_ns,
                route_revision: self.route_revision,
            });
            events.schedule(
                arrived_ns,
                20,
                Event::Arrive {
                    packet: packet_index,
                    node: link.to.clone(),
                },
            )?;
        }
        if !self.beams[beam_index].pending.is_empty() {
            if let Some(next) = at_ns.checked_add(frame_duration_ns) {
                self.ensure_beam_frame(beam_index, next, events)?;
            }
        }
        Ok(())
    }

    fn contact_transition(
        &mut self,
        at_ns: u64,
        contact_index: usize,
        opened: bool,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let contact = &self.scenario.contacts[contact_index];
        let link_index = self
            .scenario
            .links
            .iter()
            .position(|link| link.id == contact.link_id)
            .ok_or_else(|| format!("unknown contact link: {}", contact.link_id))?;
        let state = &mut self.links[link_index];
        state.contact_up = opened;
        state.active_contact = if opened { Some(contact_index) } else { None };
        state.contact_delay_ns = if opened {
            contact.propagation_delay_ns
        } else {
            None
        };
        if !self.links[link_index].effective_up() {
            self.discard_beam_link(link_index, at_ns)?;
        }
        self.control_events.push(ControlEvent {
            at_ns,
            kind: if opened {
                "contact_open".into()
            } else {
                "contact_close".into()
            },
            link_id: Some(contact.link_id.clone()),
            route_revision: self.route_revision,
        });
        if let Some(at) = at_ns.checked_add(self.scenario.control_delay_ns) {
            if at < self.scenario.duration_ns {
                events.schedule(at, 5, Event::InstallRoutes)?;
            }
        }
        Ok(())
    }

    fn fault(
        &mut self,
        at_ns: u64,
        fault_index: usize,
        events: &mut Scheduler<Event>,
    ) -> Result<(), String> {
        let fault = &self.scenario.faults[fault_index];
        let index = self
            .scenario
            .links
            .iter()
            .position(|link| link.id == fault.link_id)
            .ok_or_else(|| format!("unknown link: {}", fault.link_id))?;
        self.links[index].admin_up = matches!(fault.action, LinkAction::Up);
        if !self.links[index].effective_up() {
            self.discard_beam_link(index, at_ns)?;
        }
        self.control_events.push(ControlEvent {
            at_ns,
            kind: if self.links[index].admin_up {
                "link_up".into()
            } else {
                "link_down".into()
            },
            link_id: Some(fault.link_id.clone()),
            route_revision: self.route_revision,
        });
        if let Some(install_at) = at_ns.checked_add(self.scenario.control_delay_ns) {
            if install_at < self.scenario.duration_ns {
                events.schedule(install_at, 5, Event::InstallRoutes)?;
            }
        }
        Ok(())
    }
}

/// Apply deterministic PAT acquisition before the packet event engine runs.
/// Only opted-in optical links are gated; other contacts are unchanged.
/// Administrative down/up transitions interrupt tracking, release optical
/// heads and require a fresh acquisition before forwarding can resume.
/// Replay sampled commands against the accepted SYS-03 reservation timeline.
/// PAT planning uses a provisional unleased follower pass to compute boundary
/// projections; this second pass verifies the identical actuator history while
/// acquiring and releasing link-scoped command leases at virtual-time edges.
/// End, start and sample transitions at the same instant run in that order.
fn replay_pat_head_leases(
    scenario: &Scenario,
    reservations: &[PatReservation],
    expected: &PredictedObservations,
) -> Result<BodyHeadRuntime, String> {
    // SYS-03 validates the entire lease schedule and merges the sampled
    // command edges before SYS-02 mutates any follower or command lease.
    let samples: Vec<_> = scenario
        .optical_body_following_samples
        .iter()
        .map(|sample| (sample.at_ns, sample.link_id.as_str()))
        .collect();
    let timeline = pat_runtime_transitions(reservations, &samples)?;

    let pat_links: HashMap<_, _> = scenario
        .optical_pat
        .iter()
        .map(|pat| (pat.link_id.as_str(), pat))
        .collect();
    let mut runtime = BodyHeadRuntime::new(&scenario.optical_head_slew_actuators)?;
    let mut active_links = HashSet::new();
    for event in timeline {
        match event.kind {
            PatRuntimeTransitionKind::Release(index) => {
                let reservation = &reservations[index];
                if !active_links.remove(&reservation.link_id) {
                    return Err("PAT head reservation ended without an active lease".into());
                }
                runtime.release_pair_at(
                    &reservation.link_id,
                    &reservation.from_head,
                    &reservation.to_head,
                    reservation.end_ns,
                )?;
            }
            PatRuntimeTransitionKind::Reserve(index) => {
                let reservation = &reservations[index];
                if reservation.start_ns >= reservation.end_ns
                    || !active_links.insert(reservation.link_id.clone())
                {
                    return Err("invalid or overlapping PAT head reservation".into());
                }
                runtime.reserve_pair_for_link(
                    &reservation.link_id,
                    &reservation.from_head,
                    &reservation.to_head,
                    reservation.start_ns,
                )?;
            }
            PatRuntimeTransitionKind::Sample(index) => {
                let sample = &scenario.optical_body_following_samples[index];
                let pat = pat_links
                    .get(sample.link_id.as_str())
                    .ok_or("unknown predicted PAT link")?;
                let observed = if active_links.contains(&sample.link_id) {
                    runtime.observe_pair_for_link(
                        &sample.link_id,
                        &pat.from_head,
                        &pat.to_head,
                        sample.at_ns,
                        sample.from_target_body,
                        sample.to_target_body,
                    )
                } else {
                    // A sample during pointing/admin loss still updates the
                    // exclusive head's held target for later reacquisition.
                    // Shared-head predicted streams remain contract-rejected.
                    runtime.observe_pair(
                        &pat.from_head,
                        &pat.to_head,
                        sample.at_ns,
                        sample.from_target_body,
                        sample.to_target_body,
                    )
                };
                let provisional = expected
                    .get(&(sample.link_id.clone(), sample.at_ns))
                    .ok_or("missing provisional BODY-frame observation")?;
                if &observed != provisional {
                    return Err("PAT lease replay diverged from predicted head history".into());
                }
            }
        }
    }
    Ok(runtime)
}

fn prepare_pat(scenario: &Scenario) -> Result<(Scenario, Vec<PatEventTrace>), String> {
    let mut prepared = scenario.clone();
    if scenario.optical_pat.is_empty() {
        return Ok((prepared, Vec::new()));
    }
    let configs: HashMap<_, _> = scenario
        .optical_pat
        .iter()
        .map(|config| (config.link_id.as_str(), config))
        .collect();

    let pointing_by_link: HashMap<_, _> = scenario
        .optical_pointing
        .iter()
        .map(|pointing| (pointing.link_id.as_str(), pointing))
        .collect();
    let geometry_by_contact: HashMap<_, _> = scenario
        .optical_geometry
        .iter()
        .map(|geometry| ((geometry.link_id.as_str(), geometry.at_ns), geometry))
        .collect();
    let mut pointing_rejections = Vec::new();
    let slew_targets_by_contact: HashMap<_, _> = scenario
        .optical_pat_slew_targets
        .iter()
        .map(|target| ((target.link_id.as_str(), target.at_ns), target))
        .collect();

    // SYS-02 samples are interpreted inside Rust at virtual-time boundaries.
    // Only effective state edges reach the packet engine; these observations
    // are never stored as administrative faults in the input scenario.
    let mut steering_faults: Vec<Fault> = scenario
        .optical_steering_samples
        .iter()
        .map(|sample| Fault {
            at_ns: sample.at_ns,
            link_id: sample.link_id.clone(),
            action: if sample.from_off_axis_deg <= sample.from_max_off_axis_deg
                && sample.to_off_axis_deg <= sample.to_max_off_axis_deg
            {
                LinkAction::Up
            } else {
                LinkAction::Down
            },
            cause: FaultCause::SampledPointing,
        })
        .collect();
    // A matching measured boresight alone cannot certify a target jump
    // that exceeds either head's slew-rate budget between observations.
    let body_head_rates: HashMap<_, _> = scenario
        .optical_head_slew_actuators
        .iter()
        .map(|head| (head.head_id.as_str(), head.max_slew_rate_deg_per_s))
        .collect();
    let body_pat_links: HashMap<_, _> = scenario
        .optical_pat
        .iter()
        .map(|pat| (pat.link_id.as_str(), pat))
        .collect();
    let mut previous_body_sample = HashMap::new();
    steering_faults.extend(scenario.optical_body_tracking_samples.iter().map(|sample| {
        let previous = previous_body_sample.insert(sample.link_id.as_str(), sample);
        let rate_allowed = previous.is_none_or(|earlier| {
            let pat = body_pat_links[sample.link_id.as_str()];
            body_tracking_target_rate_allowed(
                earlier,
                sample,
                body_head_rates[pat.from_head.as_str()],
                body_head_rates[pat.to_head.as_str()],
            )
        });
        Fault {
            at_ns: sample.at_ns,
            link_id: sample.link_id.clone(),
            action: if body_tracking_error_allowed(sample) && rate_allowed {
                LinkAction::Up
            } else {
                LinkAction::Down
            },
            cause: FaultCause::SampledPointing,
        }
    }));
    // SYS-02 owns the mutable per-head follower state and sample history.
    // PAT receives only immutable projections at candidate boundaries.
    let mut follower_runtime = (!scenario.optical_body_following_samples.is_empty())
        .then(|| BodyHeadRuntime::new(&scenario.optical_head_slew_actuators))
        .transpose()?;
    let mut predicted_observations = HashMap::new();
    if let Some(runtime) = follower_runtime.as_mut() {
        let mut commands: Vec<_> = scenario.optical_body_following_samples.iter().collect();
        commands.sort_by(|a, b| (a.at_ns, &a.link_id).cmp(&(b.at_ns, &b.link_id)));
        for command in commands {
            let pat = body_pat_links[command.link_id.as_str()];
            let observed = runtime.observe_pair(
                &pat.from_head,
                &pat.to_head,
                command.at_ns,
                command.from_target_body,
                command.to_target_body,
            );
            predicted_observations
                .insert((command.link_id.clone(), command.at_ns), observed.clone());
            let allowed = match observed {
                Ok((from_boresight_body, to_boresight_body)) => {
                    body_tracking_error_allowed(&OpticalBodyTrackingSample {
                        link_id: command.link_id.clone(),
                        at_ns: command.at_ns,
                        frame: command.frame.clone(),
                        from_target_body: command.from_target_body,
                        to_target_body: command.to_target_body,
                        from_boresight_body,
                        to_boresight_body,
                        from_max_error_deg: command.from_max_error_deg,
                        to_max_error_deg: command.to_max_error_deg,
                    })
                }
                _ => false,
            };
            steering_faults.push(Fault {
                at_ns: command.at_ns,
                link_id: command.link_id.clone(),
                action: if allowed {
                    LinkAction::Up
                } else {
                    LinkAction::Down
                },
                cause: FaultCause::SampledPointing,
            });
        }
    }
    // Compute reciprocal ITRS line-of-sight geometry in Rust at each observation.
    // These supplied frame-tagged vectors are held between samples; no live
    // orbit or attitude integration occurs inside the packet runtime.
    let tracking_rates: HashMap<_, _> = scenario
        .optical_tracking_rates
        .iter()
        .map(|rate| (rate.link_id.as_str(), rate))
        .collect();
    let tracking_accelerations: HashMap<_, _> = scenario
        .optical_tracking_accelerations
        .iter()
        .map(|acceleration| (acceleration.link_id.as_str(), acceleration))
        .collect();
    let mut previous_geometry: HashMap<
        &str,
        (
            &ntarlink_contracts::OpticalGeometry,
            Option<&ntarlink_contracts::OpticalGeometry>,
        ),
    > = HashMap::new();
    steering_faults.extend(scenario.optical_geometry_samples.iter().map(|sample| {
        let previous = previous_geometry.get(sample.link_id.as_str()).copied();
        previous_geometry.insert(
            sample.link_id.as_str(),
            (sample, previous.map(|(earlier, _)| earlier)),
        );
        let rate_allowed = tracking_rates
            .get(sample.link_id.as_str())
            .is_none_or(|rate| {
                previous.is_none_or(|(earlier, _)| {
                    geometry_los_rate_allowed(
                        earlier,
                        sample,
                        rate.from_max_los_rate_deg_per_s
                            .min(rate.to_max_los_rate_deg_per_s),
                    )
                })
            });
        let acceleration_allowed = tracking_accelerations
            .get(sample.link_id.as_str())
            .is_none_or(|acceleration| {
                previous.is_none_or(|(earlier, first)| {
                    first.is_none_or(|first| {
                        geometry_los_acceleration_allowed(
                            first,
                            earlier,
                            sample,
                            acceleration
                                .from_max_los_accel_deg_per_s2
                                .min(acceleration.to_max_los_accel_deg_per_s2),
                        )
                    })
                })
            });
        Fault {
            at_ns: sample.at_ns,
            link_id: sample.link_id.clone(),
            action: if geometry_pointing_allowed(sample) && rate_allowed && acceleration_allowed {
                LinkAction::Up
            } else {
                LinkAction::Down
            },
            cause: FaultCause::SampledPointing,
        }
    }));

    // Administrative and sampled-pointing faults are independent state domains.
    // Fold all edges at the same timestamp before evaluating effective availability:
    // a recovery in one domain must never reopen a still-failed other domain.
    let mut faults_by_link: HashMap<&str, Vec<_>> = HashMap::new();
    for (index, fault) in scenario.faults.iter().enumerate() {
        if configs.contains_key(fault.link_id.as_str()) {
            faults_by_link
                .entry(fault.link_id.as_str())
                .or_default()
                .push((index, fault));
        }
    }
    for (index, fault) in steering_faults.iter().enumerate() {
        faults_by_link
            .entry(fault.link_id.as_str())
            .or_default()
            .push((scenario.faults.len() + index, fault));
    }
    for faults in faults_by_link.values_mut() {
        faults.sort_by_key(|(index, fault)| (fault.at_ns, *index));
    }

    let mut admin_windows: HashMap<&str, Vec<(u64, u64)>> = HashMap::new();
    let mut down_faults: HashMap<(&str, u64), FaultCause> = HashMap::new();
    let mut effective_faults: Vec<Fault> = Vec::new();
    for config in &scenario.optical_pat {
        let link = scenario
            .links
            .iter()
            .find(|link| link.id == config.link_id)
            .expect("validated optical PAT link");
        let mut windows = Vec::new();
        let mut cursor = 0;
        let mut admin_up = link.up;
        let mut pointing_up = true;
        if let Some(faults) = faults_by_link.get(config.link_id.as_str()) {
            let mut index = 0;
            while index < faults.len() {
                let at_ns = faults[index].1.at_ns;
                let was_up = admin_up && pointing_up;
                while index < faults.len() && faults[index].1.at_ns == at_ns {
                    let fault = faults[index].1;
                    match fault.cause {
                        FaultCause::Administrative => {
                            admin_up = matches!(fault.action, LinkAction::Up);
                        }
                        FaultCause::SampledPointing => {
                            pointing_up = matches!(fault.action, LinkAction::Up);
                        }
                    }
                    index += 1;
                }
                let now_up = admin_up && pointing_up;
                if was_up != now_up {
                    if was_up && cursor < at_ns {
                        windows.push((cursor, at_ns));
                    }
                    if !now_up {
                        down_faults.insert(
                            (config.link_id.as_str(), at_ns),
                            if !admin_up {
                                FaultCause::Administrative
                            } else {
                                FaultCause::SampledPointing
                            },
                        );
                    }
                    cursor = at_ns;
                    // The packet engine consumes the *effective* state, not
                    // independent raw fault edges that could reopen it early.
                    effective_faults.push(Fault {
                        at_ns,
                        link_id: config.link_id.clone(),
                        action: if now_up {
                            LinkAction::Up
                        } else {
                            LinkAction::Down
                        },
                        cause: FaultCause::Administrative,
                    });
                }
            }
        }
        if admin_up && pointing_up && cursor < scenario.duration_ns {
            windows.push((cursor, scenario.duration_ns));
        }
        admin_windows.insert(config.link_id.as_str(), windows);
    }

    let availability = HeadAvailability::new(&scenario.optical_head_outages);
    let mut head_downs = HashSet::new();
    let mut candidates = Vec::new();
    let mut candidate_slew_targets = Vec::new();
    let mut candidate_slew_starts = Vec::new();
    // Candidate end times are unique per directed link because both geometric
    // contacts and administrative intervals are non-overlapping.
    let mut source_by_end = HashMap::new();
    for (source_index, contact) in scenario.contacts.iter().enumerate() {
        let Some(config) = configs.get(contact.link_id.as_str()) else {
            continue;
        };
        let windows = admin_windows
            .get(contact.link_id.as_str())
            .expect("validated optical PAT configuration");
        let first = windows.partition_point(|(_, end)| *end <= contact.start_ns);
        for &(up_start, up_end) in &windows[first..] {
            if up_start >= contact.end_ns {
                break;
            }
            let start_ns = contact.start_ns.max(up_start);
            let end_ns = contact.end_ns.min(up_end);
            if start_ns >= end_ns {
                continue;
            }
            // SYS-02 rejects a candidate if either assumed off-boresight
            // angle exceeds its configured head steering envelope.
            if let Some(pointing) = pointing_by_link.get(contact.link_id.as_str()) {
                if !pointing_allowed(pointing) {
                    pointing_rejections.push(PatEventTrace {
                        link_id: contact.link_id.clone(),
                        at_ns: start_ns,
                        state: "rejected",
                        reason: Some("pointing_outside_envelope"),
                    });
                    continue;
                }
            }
            // SYS-02 frame-tagged ITRS snapshot at the geometric contact start.
            // The supplied boresight is held fixed throughout this contact.
            if let Some(geometry) =
                geometry_by_contact.get(&(contact.link_id.as_str(), contact.start_ns))
            {
                if !geometry_pointing_allowed(geometry) {
                    pointing_rejections.push(PatEventTrace {
                        link_id: contact.link_id.clone(),
                        at_ns: start_ns,
                        state: "rejected",
                        reason: Some("geometry_outside_envelope"),
                    });
                    continue;
                }
            }
            // SYS-02 terminal outages further partition admin-up geometric
            // contacts. Each recovered segment must reacquire both heads.
            for (admitted_start, admitted_end) in availability.available_intervals(
                &config.from_head,
                &config.to_head,
                start_ns,
                end_ns,
            ) {
                if candidates.len() >= ntarlink_optical_pat::MAX_CANDIDATES {
                    return Err("PAT candidate budget exceeded".into());
                }
                if admitted_end < end_ns
                    && (availability.outage_starts_at(&config.from_head, admitted_end)
                        || availability.outage_starts_at(&config.to_head, admitted_end))
                {
                    head_downs.insert((contact.link_id.as_str(), admitted_end));
                }
                candidates.push(PatCandidate {
                    link_id: contact.link_id.clone(),
                    from_head: config.from_head.clone(),
                    to_head: config.to_head.clone(),
                    start_ns: admitted_start,
                    end_ns: admitted_end,
                    slew_ns: config.slew_ns,
                    acquisition_ns: config.acquisition_ns,
                });
                if !scenario.optical_body_following_samples.is_empty() {
                    // Query an immutable follower snapshot at this candidate boundary.
                    // This does not introduce a new rest-to-rest sample into its history.
                    let runtime = follower_runtime
                        .as_ref()
                        .ok_or("missing predicted optical head runtime")?;
                    let (from_direction, from_target) =
                        runtime.predicted_at(&config.from_head, admitted_start)?;
                    let (to_direction, to_target) =
                        runtime.predicted_at(&config.to_head, admitted_start)?;
                    candidate_slew_starts.push(PatSlewStart {
                        link_id: contact.link_id.clone(),
                        start_ns: admitted_start,
                        from_direction_body: from_direction,
                        to_direction_body: to_direction,
                    });
                    candidate_slew_targets.push(PatSlewTarget {
                        link_id: contact.link_id.clone(),
                        start_ns: admitted_start,
                        from_target_body: from_target,
                        to_target_body: to_target,
                    });
                } else if !scenario.optical_head_slew_actuators.is_empty() {
                    let target = slew_targets_by_contact
                        .get(&(contact.link_id.as_str(), contact.start_ns))
                        .ok_or("missing validated body-frame PAT target")?;
                    candidate_slew_targets.push(PatSlewTarget {
                        link_id: contact.link_id.clone(),
                        start_ns: admitted_start,
                        from_target_body: target.from_target_body,
                        to_target_body: target.to_target_body,
                    });
                }
                source_by_end.insert((contact.link_id.clone(), admitted_end), source_index);
            }
        }
    }
    let plan = if scenario.optical_head_slew_actuators.is_empty() {
        plan_contacts(&candidates)?
    } else {
        let actuators: Vec<_> = scenario
            .optical_head_slew_actuators
            .iter()
            .map(|actuator| HeadSlewActuator {
                head_id: actuator.head_id.clone(),
                initial_direction_body: actuator.initial_direction_body,
                max_slew_rate_deg_per_s: actuator.max_slew_rate_deg_per_s,
                max_slew_acceleration_deg_per_s2: actuator.max_slew_acceleration_deg_per_s2,
                settling_ns: actuator.settling_ns,
            })
            .collect();
        if candidate_slew_starts.is_empty() {
            plan_contacts_with_slew(&candidates, &actuators, &candidate_slew_targets)?
        } else {
            plan_contacts_with_slew_starts(
                &candidates,
                &actuators,
                &candidate_slew_targets,
                &candidate_slew_starts,
            )?
        }
    };
    if follower_runtime.is_some() {
        // Verify lease acquisition/release against the actual PAT reservations.
        // The provisional pass remains the source of immutable PAT projections.
        replay_pat_head_leases(scenario, &plan.reservations, &predicted_observations)?;
    }
    let mut windows_by_source: HashMap<usize, Vec<_>> = HashMap::new();
    for window in &plan.windows {
        let source = source_by_end
            .get(&(window.link_id.clone(), window.end_ns))
            .ok_or("PAT acquired window has no geometric source")?;
        windows_by_source.entry(*source).or_default().push(window);
    }
    prepared.contacts = scenario
        .contacts
        .iter()
        .enumerate()
        .flat_map(|(index, contact)| {
            if !configs.contains_key(contact.link_id.as_str()) {
                return vec![contact.clone()];
            }
            windows_by_source
                .get(&index)
                .into_iter()
                .flatten()
                .map(|window| {
                    let mut acquired = contact.clone();
                    acquired.start_ns = window.start_ns;
                    acquired.end_ns = window.end_ns;
                    if !contact.propagation_delay_samples.is_empty() {
                        let original = &contact.propagation_delay_samples;
                        let mut samples = vec![PropagationDelaySample {
                            at_ns: window.start_ns,
                            delay_ns: interpolated_delay(original, window.start_ns),
                        }];
                        samples.extend(
                            original
                                .iter()
                                .filter(|sample| {
                                    sample.at_ns > window.start_ns
                                        && sample.at_ns < window.end_ns - 1
                                })
                                .cloned(),
                        );
                        if window.end_ns - 1 > window.start_ns {
                            samples.push(PropagationDelaySample {
                                at_ns: window.end_ns - 1,
                                delay_ns: interpolated_delay(original, window.end_ns - 1),
                            });
                        }
                        acquired.propagation_delay_samples = samples;
                    }
                    acquired
                })
                .collect::<Vec<_>>()
        })
        .collect();
    // These contacts are already PAT-gated; validate them as packet contacts.
    // Keeping the original PAT configuration would require rejected contacts to
    // remain present and would incorrectly reject fully unacquirable links.
    prepared.optical_pat.clear();
    // The prepared packet scenario no longer owns PAT metadata. Replace
    // overlapping domain-specific PAT faults with effective transitions so
    // control events and route installation never report a false recovery.
    prepared
        .faults
        .retain(|fault| !configs.contains_key(fault.link_id.as_str()));
    prepared.faults.extend(effective_faults);
    prepared.optical_head_slew_actuators.clear();
    prepared.optical_pat_slew_targets.clear();
    prepared.optical_body_tracking_samples.clear();
    prepared.optical_body_following_samples.clear();
    prepared.optical_head_outages.clear();
    prepared.optical_pointing.clear();
    prepared.optical_steering_samples.clear();
    prepared.optical_geometry_samples.clear();
    prepared.optical_tracking_rates.clear();
    prepared.optical_tracking_accelerations.clear();
    prepared.optical_geometry.clear();
    let mut events: Vec<PatEventTrace> = plan
        .events
        .into_iter()
        .map(|event| {
            let reason = if event.state == PatState::Lost
                && down_faults.contains_key(&(event.link_id.as_str(), event.at_ns))
            {
                Some(match down_faults[&(event.link_id.as_str(), event.at_ns)] {
                    FaultCause::Administrative => "administrative_link_down",
                    FaultCause::SampledPointing => "sampled_pointing_loss",
                })
            } else if event.state == PatState::Lost
                && head_downs.contains(&(event.link_id.as_str(), event.at_ns))
            {
                Some("optical_head_unavailable")
            } else {
                event.reason
            };
            PatEventTrace {
                link_id: event.link_id,
                at_ns: event.at_ns,
                state: match event.state {
                    PatState::Candidate => "candidate",
                    PatState::Slewing => "slewing",
                    PatState::Acquiring => "acquiring",
                    PatState::Tracking => "tracking",
                    PatState::Lost => "lost",
                    PatState::Rejected => "rejected",
                },
                reason,
            }
        })
        .collect();
    events.extend(pointing_rejections);
    events.sort_by(|a, b| {
        let order = |state: &str| match state {
            "lost" => 0,
            "candidate" => 1,
            "slewing" => 2,
            "acquiring" => 3,
            "tracking" => 4,
            _ => 5,
        };
        (a.at_ns, order(a.state), &a.link_id).cmp(&(b.at_ns, order(b.state), &b.link_id))
    });
    Ok((prepared, events))
}

/// Run a deterministic packet simulation with optional modeled optical PAT gates.
/// PAT events are planned, not physical pointing or measured link state.
pub fn run(scenario: &Scenario) -> Result<SimulationReport, String> {
    scenario.validate()?;
    let (prepared, pat_events) = prepare_pat(scenario)?;
    let mut report = run_inner(&prepared)?;
    if !scenario.optical_pat.is_empty() {
        report.fidelity = "l1_scheduled_contact_pat_gate_packet_model".into();
    }
    report.pat_events = pat_events;
    Ok(report)
}

/// Execute the existing L1 packet event engine against admitted contact windows.
fn run_inner(scenario: &Scenario) -> Result<SimulationReport, String> {
    scenario.validate()?;
    let mut state = Runtime::new(scenario);
    let mut scheduler = Scheduler::new();
    for (index, fault) in scenario.faults.iter().enumerate() {
        scheduler.schedule(fault.at_ns, 0, Event::Fault { index })?;
    }
    for (index, window) in scenario.contacts.iter().enumerate() {
        if window.start_ns > 0 {
            scheduler.schedule(window.start_ns, 1, Event::ContactOpen { index })?;
        }
        if window.end_ns < scenario.duration_ns {
            scheduler.schedule(window.end_ns, 0, Event::ContactClose { index })?;
        }
    }
    for (flow_index, flow) in scenario.flows.iter().enumerate() {
        for ordinal in 0..flow.packet_count {
            let at_ns = flow.start_ns + u64::from(ordinal) * flow.interval_ns;
            scheduler.schedule(
                at_ns,
                10,
                Event::Inject {
                    flow: flow_index,
                    ordinal,
                },
            )?;
        }
    }
    let mut processed_events = 0u64;
    while let Some(scheduled) = scheduler.next_event() {
        processed_events += 1;
        if processed_events > MAX_PROCESSED_EVENTS {
            return Err("processed event budget exceeded".into());
        }
        match scheduled.event {
            Event::Inject { flow, ordinal } => {
                state.inject(scheduled.at_ns, flow, ordinal, &mut scheduler)?;
            }
            Event::Arrive { packet, node } => {
                state.arrive(scheduled.at_ns, packet, node, &mut scheduler)?;
            }
            Event::Fault { index } => {
                state.fault(scheduled.at_ns, index, &mut scheduler)?;
            }
            Event::ContactOpen { index } => {
                state.contact_transition(scheduled.at_ns, index, true, &mut scheduler)?;
            }
            Event::ContactClose { index } => {
                state.contact_transition(scheduled.at_ns, index, false, &mut scheduler)?;
            }
            Event::InstallRoutes => state.install_routes(scheduled.at_ns),
            Event::BeamFrame { beam } => {
                state.beam_frame(scheduled.at_ns, beam, &mut scheduler)?;
            }
        }
    }
    // Queued grants beyond the scenario horizon cannot be delivered.
    let remaining: Vec<usize> = state
        .beams
        .iter()
        .flat_map(|beam| beam.pending.keys().copied())
        .collect();
    for packet_index in remaining {
        state.drop_packet(packet_index, scenario.duration_ns, "simulation_horizon");
    }
    let packets: Vec<PacketTrace> = state.packets.into_iter().map(|p| p.trace).collect();
    let delivered = packets
        .iter()
        .filter(|p| p.status == PacketStatus::Delivered)
        .count();
    let dropped = packets
        .iter()
        .filter(|p| p.status == PacketStatus::Dropped)
        .count();
    if delivered + dropped != packets.len() {
        return Err("packet accounting invariant failed".into());
    }
    Ok(SimulationReport {
        scenario: scenario.name.clone(),
        fidelity: if scenario.contact_mode == ContactMode::Scheduled {
            "l1_sampled_orbital_contact_packet_model".into()
        } else if scenario.contacts.is_empty() {
            "l1_static_contact_packet_model".into()
        } else {
            "l1_scheduled_contact_fixture_packet_model".into()
        },
        duration_ns: scenario.duration_ns,
        processed_events,
        route_revision: state.route_revision,
        injected: packets.len(),
        delivered,
        dropped,
        packets,
        control_events: state.control_events,
        beam_grants: state.beam_grants,
        pat_events: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntarlink_contracts::Scenario;

    fn basic() -> Scenario {
        Scenario::from_json(include_str!("../../../scenarios/minimal.json")).unwrap()
    }

    #[test]
    fn reaches_internet_through_all_five_link_types() {
        let result = run(&basic()).unwrap();
        assert_eq!(result.injected, 3);
        assert_eq!(result.delivered, 3);
        assert_eq!(result.dropped, 0);
        for packet in &result.packets {
            let actual: Vec<_> = packet.hops.iter().map(|h| h.link_id.as_str()).collect();
            assert_eq!(
                actual,
                [
                    "user-ue-a",
                    "isl-a-b",
                    "feeder-b-gw",
                    "backhaul-gw-pop",
                    "pop-internet"
                ]
            );
            let total: u64 = packet
                .hops
                .iter()
                .map(|h| h.queue_delay_ns + h.serialization_ns + h.propagation_ns + h.processing_ns)
                .sum();
            assert_eq!(packet.completed_ns.unwrap() - packet.injected_ns, total);
        }
    }

    fn pat_fixture() -> Scenario {
        Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap()
    }

    #[test]
    fn pat_head_reservations_cover_pretracking_dwell_and_match_packet_windows() {
        let scenario = pat_fixture();
        let config = &scenario.optical_pat[0];
        let candidates: Vec<_> = scenario
            .contacts
            .iter()
            .filter(|contact| contact.link_id == config.link_id)
            .map(|contact| PatCandidate {
                link_id: contact.link_id.clone(),
                from_head: config.from_head.clone(),
                to_head: config.to_head.clone(),
                start_ns: contact.start_ns,
                end_ns: contact.end_ns,
                slew_ns: config.slew_ns,
                acquisition_ns: config.acquisition_ns,
            })
            .collect();
        let plan = plan_contacts(&candidates).unwrap();
        assert_eq!(
            plan.reservations
                .iter()
                .map(|reservation| (reservation.start_ns, reservation.end_ns))
                .collect::<Vec<_>>(),
            vec![(0, 120_000_000), (120_000_000, 200_000_000)]
        );
        let (prepared, _) = prepare_pat(&scenario).unwrap();
        let admitted: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|contact| contact.link_id == config.link_id)
            .map(|contact| (contact.start_ns, contact.end_ns))
            .collect();
        assert_eq!(
            admitted,
            plan.windows
                .iter()
                .map(|window| (window.start_ns, window.end_ns))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            admitted,
            vec![(20_000_000, 120_000_000), (140_000_000, 200_000_000)]
        );
        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, report.delivered + report.dropped);
    }

    #[test]
    fn pat_tracking_gates_packets_and_reacquires_after_contact_boundary() {
        let scenario = pat_fixture();
        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, 7);
        assert!(report.delivered > 0 && report.dropped > 0);
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            report.fidelity,
            "l1_scheduled_contact_pat_gate_packet_model"
        );
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 140_000_000]);
        assert!(report.control_events.iter().any(|event| {
            event.kind == "contact_open"
                && event.link_id.as_deref() == Some("isl-a-b")
                && event.at_ns == 140_000_000
        }));
        let optical_hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(!optical_hops.is_empty());
        assert!(optical_hops.iter().all(|hop| {
            (20_000_000..120_000_000).contains(&hop.transmit_started_ns)
                || (140_000_000..200_000_000).contains(&hop.transmit_started_ns)
        }));
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn sampled_body_target_rate_loss_blocks_pat_even_with_perfect_boresight() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-target-rate-gate.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 90_000_000, 190_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 25_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        assert!(!report.control_events.iter().any(|event| {
            event.kind == "contact_open"
                && event.link_id.as_deref() == Some("isl-a-b")
                && (40_000_000..70_000_000).contains(&event.at_ns)
        }));
        let hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(hops.iter().all(|hop| {
            (20_000_000..25_000_000).contains(&hop.transmit_started_ns)
                || (90_000_000..120_000_000).contains(&hop.transmit_started_ns)
                || (190_000_000..260_000_000).contains(&hop.transmit_started_ns)
        }));
        assert!(hops
            .iter()
            .any(|hop| hop.transmit_started_ns >= 190_000_000));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn predicted_body_follower_lag_interrupts_pat_and_reacquires() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-target-following.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 110_000_000, 200_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        let hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(hops.iter().all(|hop| {
            (20_000_000..30_000_000).contains(&hop.transmit_started_ns)
                || (110_000_000..120_000_000).contains(&hop.transmit_started_ns)
                || (200_000_000..350_000_000).contains(&hop.transmit_started_ns)
        }));
        assert!(hops
            .iter()
            .any(|hop| hop.transmit_started_ns >= 200_000_000));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn predicted_pat_leases_release_at_fault_and_contact_boundaries() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-target-following.json"
        ))
        .unwrap();
        let mut provisional = BodyHeadRuntime::new(&scenario.optical_head_slew_actuators).unwrap();
        let mut expected = HashMap::new();
        for sample in &scenario.optical_body_following_samples {
            let pat = &scenario.optical_pat[0];
            let observed = provisional.observe_pair(
                &pat.from_head,
                &pat.to_head,
                sample.at_ns,
                sample.from_target_body,
                sample.to_target_body,
            );
            expected.insert((sample.link_id.clone(), sample.at_ns), observed);
        }
        let reservation = |start_ns, end_ns| PatReservation {
            link_id: "isl-a-b".into(),
            from_head: "sat-a/head-1".into(),
            to_head: "sat-b/head-1".into(),
            start_ns,
            end_ns,
        };
        let reservations = vec![
            reservation(0, 30_000_000),
            reservation(90_000_000, 120_000_000),
            reservation(120_000_000, 350_000_000),
        ];
        let mut replayed = replay_pat_head_leases(&scenario, &reservations, &expected).unwrap();
        let mut reversed = reservations.clone();
        reversed.reverse();
        let replayed_reversed = replay_pat_head_leases(&scenario, &reversed, &expected).unwrap();
        for sample in &scenario.optical_body_following_samples {
            for head in ["sat-a/head-1", "sat-b/head-1"] {
                assert_eq!(
                    replayed.predicted_at(head, sample.at_ns).unwrap(),
                    provisional.predicted_at(head, sample.at_ns).unwrap()
                );
                assert_eq!(
                    replayed_reversed.predicted_at(head, sample.at_ns).unwrap(),
                    provisional.predicted_at(head, sample.at_ns).unwrap()
                );
            }
        }
        assert!(replayed
            .observe("sat-a/head-1", 349_000_000, [1.0, 0.0, 0.0])
            .is_err());
        assert!(replayed
            .observe("sat-a/head-1", 350_000_000, [1.0, 0.0, 0.0])
            .is_ok());

        let mut conflicting = reservations.clone();
        conflicting.push(PatReservation {
            link_id: "other-link".into(),
            from_head: "sat-a/head-1".into(),
            to_head: "sat-b/head-1".into(),
            start_ns: 100_000_000,
            end_ns: 110_000_000,
        });
        assert!(replay_pat_head_leases(&scenario, &conflicting, &expected).is_err());

        // Duplicate link/timestamp commands are rejected before SYS-02 state
        // changes, even if a caller bypasses scenario contract validation.
        let mut duplicate = scenario.clone();
        duplicate
            .optical_body_following_samples
            .push(scenario.optical_body_following_samples[0].clone());
        assert_eq!(
            replay_pat_head_leases(&duplicate, &reservations, &expected).unwrap_err(),
            "invalid or duplicate PAT sampled head command"
        );

        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn predicted_pat_replay_rejects_mismatched_sample_failures() {
        let mut scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-target-following.json"
        ))
        .unwrap();
        scenario.optical_body_following_samples.truncate(1);
        let sample = &mut scenario.optical_body_following_samples[0];
        sample.from_target_body = [0.0, 0.0, 0.0];

        let key = (sample.link_id.clone(), sample.at_ns);
        let mut expected = HashMap::new();
        expected.insert(key.clone(), Err("invalid BODY-frame direction".to_owned()));
        let pat = &scenario.optical_pat[0];
        let reservations = vec![PatReservation {
            link_id: pat.link_id.clone(),
            from_head: pat.from_head.clone(),
            to_head: pat.to_head.clone(),
            start_ns: 0,
            end_ns: 30_000_000,
        }];

        // A genuinely invalid target is an expected sampled-pointing failure.
        assert!(replay_pat_head_leases(&scenario, &reservations, &expected).is_ok());

        // A different error must not be silently accepted merely because
        // both passes returned Err; it could indicate a lease violation.
        expected.insert(
            key,
            Err("BODY-frame runtime head owned by another link".to_owned()),
        );
        assert_eq!(
            replay_pat_head_leases(&scenario, &reservations, &expected).unwrap_err(),
            "PAT lease replay diverged from predicted head history"
        );
    }

    #[test]
    fn sampled_body_tracking_error_interrupts_and_reacquires_gimbal_pat() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-tracking-error.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 110_000_000, 190_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        assert!(!report
            .control_events
            .iter()
            .any(|event| event.kind == "contact_open"
                && event.link_id.as_deref() == Some("isl-a-b")
                && (40_000_000..90_000_000).contains(&event.at_ns)));
        let hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(!hops.is_empty());
        assert!(hops.iter().all(|hop| {
            (20_000_000..30_000_000).contains(&hop.transmit_started_ns)
                || (110_000_000..120_000_000).contains(&hop.transmit_started_ns)
                || (190_000_000..260_000_000).contains(&hop.transmit_started_ns)
        }));
        assert!(hops
            .iter()
            .any(|hop| hop.transmit_started_ns >= 190_000_000));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn body_frame_gimbal_retargeting_and_fault_split_delay_packet_admission() {
        let mut scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-body-frame-gimbal-slew.json"
        ))
        .unwrap();
        let baseline = run(&scenario).unwrap();
        let tracking = |report: &SimulationReport| {
            report
                .pat_events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>()
        };
        assert_eq!(tracking(&baseline), vec![20_000_000, 160_000_000]);
        assert_eq!(baseline.injected, baseline.delivered + baseline.dropped);
        assert_eq!(
            serde_json::to_vec(&baseline).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );

        scenario.faults = vec![
            Fault {
                at_ns: 50_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];
        let (prepared, _) = prepare_pat(&scenario).unwrap();
        prepared.validate().unwrap();
        let report = run(&scenario).unwrap();
        assert_eq!(tracking(&report), vec![20_000_000, 90_000_000, 160_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.state == "lost"
                && event.at_ns == 50_000_000
                && event.reason == Some("administrative_link_down")
        }));
        assert!(report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .all(|hop| {
                (20_000_000..50_000_000).contains(&hop.transmit_started_ns)
                    || (90_000_000..120_000_000).contains(&hop.transmit_started_ns)
                    || (160_000_000..200_000_000).contains(&hop.transmit_started_ns)
            }));
        assert_eq!(report.injected, report.delivered + report.dropped);
    }

    #[test]
    fn optical_admin_fault_releases_tracking_and_reacquires_after_recovery() {
        use ntarlink_contracts::Fault;
        let mut scenario = pat_fixture();
        scenario.faults = vec![
            Fault {
                at_ns: 50_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];
        scenario.flows[0].packet_count = 18;
        scenario.flows[0].interval_ns = 10_000_000;
        let (prepared, _) = prepare_pat(&scenario).unwrap();
        let optical: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|contact| contact.link_id == "isl-a-b")
            .map(|contact| (contact.start_ns, contact.end_ns))
            .collect();
        assert_eq!(
            optical,
            vec![
                (20_000_000, 50_000_000),
                (90_000_000, 120_000_000),
                (140_000_000, 200_000_000)
            ]
        );
        prepared.validate().unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 90_000_000, 140_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 50_000_000
                && event.state == "lost"
                && event.reason == Some("administrative_link_down")
        }));
        assert!(report.control_events.iter().any(|event| {
            event.at_ns == 90_000_000
                && event.kind == "contact_open"
                && event.link_id.as_deref() == Some("isl-a-b")
        }));
        assert!(report
            .control_events
            .iter()
            .any(|event| { event.at_ns == 95_000_000 && event.kind == "routes_installed" }));
        let optical_hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(optical_hops.iter().all(|hop| optical
            .iter()
            .any(|(start, end)| { (*start..*end).contains(&hop.transmit_started_ns) })));
        assert!(optical_hops
            .iter()
            .any(|hop| (90_000_000..120_000_000).contains(&hop.transmit_started_ns)));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn sampled_pointing_loss_has_distinct_pat_reason_and_reacquires() {
        use ntarlink_contracts::Fault;
        let mut scenario = pat_fixture();
        scenario.faults = vec![
            Fault {
                at_ns: 30_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::SampledPointing,
            },
            Fault {
                at_ns: 90_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::SampledPointing,
            },
        ];
        let report = run(&scenario).unwrap();
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        assert_eq!(
            report
                .pat_events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![20_000_000, 110_000_000, 140_000_000]
        );
        assert!(report
            .control_events
            .iter()
            .any(|event| { event.kind == "link_down" && event.at_ns == 30_000_000 }));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn overlapping_fault_domains_only_reacquire_after_both_recover() {
        for (first_cause, second_cause, expected_reason) in [
            (
                FaultCause::Administrative,
                FaultCause::SampledPointing,
                "administrative_link_down",
            ),
            (
                FaultCause::SampledPointing,
                FaultCause::Administrative,
                "sampled_pointing_loss",
            ),
        ] {
            let mut scenario = pat_fixture();
            scenario.faults = vec![
                Fault {
                    at_ns: 30_000_000,
                    link_id: "isl-a-b".into(),
                    action: LinkAction::Down,
                    cause: first_cause,
                },
                Fault {
                    at_ns: 40_000_000,
                    link_id: "isl-a-b".into(),
                    action: LinkAction::Down,
                    cause: second_cause,
                },
                Fault {
                    at_ns: 50_000_000,
                    link_id: "isl-a-b".into(),
                    action: LinkAction::Up,
                    cause: first_cause,
                },
                Fault {
                    at_ns: 70_000_000,
                    link_id: "isl-a-b".into(),
                    action: LinkAction::Up,
                    cause: second_cause,
                },
            ];
            scenario.flows[0].packet_count = 18;
            scenario.flows[0].interval_ns = 10_000_000;
            let (prepared, _) = prepare_pat(&scenario).unwrap();
            prepared.validate().unwrap();
            let windows: Vec<_> = prepared
                .contacts
                .iter()
                .filter(|c| c.link_id == "isl-a-b")
                .map(|c| (c.start_ns, c.end_ns))
                .collect();
            assert_eq!(
                windows,
                vec![
                    (20_000_000, 30_000_000),
                    (90_000_000, 120_000_000),
                    (140_000_000, 200_000_000)
                ]
            );
            let report = run(&scenario).unwrap();
            assert!(report.pat_events.iter().any(|event| {
                event.at_ns == 30_000_000
                    && event.state == "lost"
                    && event.reason == Some(expected_reason)
            }));
            assert_eq!(
                report
                    .pat_events
                    .iter()
                    .filter(|event| event.state == "tracking")
                    .map(|event| event.at_ns)
                    .collect::<Vec<_>>(),
                vec![20_000_000, 90_000_000, 140_000_000]
            );
            let fault_events: Vec<_> = report
                .control_events
                .iter()
                .filter(|event| {
                    event.link_id.as_deref() == Some("isl-a-b")
                        && (event.kind == "link_up" || event.kind == "link_down")
                })
                .map(|event| (event.at_ns, event.kind.as_str()))
                .collect();
            assert_eq!(
                fault_events,
                vec![(30_000_000, "link_down"), (70_000_000, "link_up")]
            );
            assert_eq!(report.injected, report.delivered + report.dropped);
            assert_eq!(
                serde_json::to_vec(&report).unwrap(),
                serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
            );
        }
    }

    #[test]
    fn native_steering_samples_gate_tracking_without_overriding_admin_faults() {
        use ntarlink_contracts::{Fault, OpticalSteeringSample};
        let mut scenario = pat_fixture();
        let sample = OpticalSteeringSample {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            from_off_axis_deg: 10.0,
            to_off_axis_deg: 10.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        };
        scenario.optical_steering_samples = vec![
            sample.clone(),
            OpticalSteeringSample {
                at_ns: 30_000_000,
                from_off_axis_deg: 40.0,
                ..sample.clone()
            },
            OpticalSteeringSample {
                at_ns: 50_000_000,
                ..sample
            },
        ];
        scenario.faults = vec![
            Fault {
                at_ns: 40_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];
        scenario.flows[0].packet_count = 18;
        scenario.flows[0].interval_ns = 10_000_000;
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|e| e.state == "tracking")
            .map(|e| e.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 90_000_000, 140_000_000]);
        assert!(report.pat_events.iter().any(|e| {
            e.at_ns == 30_000_000 && e.state == "lost" && e.reason == Some("sampled_pointing_loss")
        }));
        let fault_edges: Vec<_> = report
            .control_events
            .iter()
            .filter(|e| {
                e.link_id.as_deref() == Some("isl-a-b")
                    && (e.kind == "link_down" || e.kind == "link_up")
            })
            .map(|e| (e.at_ns, e.kind.as_str()))
            .collect();
        assert_eq!(
            fault_edges,
            vec![(30_000_000, "link_down"), (70_000_000, "link_up")]
        );
        assert!(report
            .packets
            .iter()
            .flat_map(|p| &p.hops)
            .filter(|h| h.link_id == "isl-a-b")
            .all(|h| !(30_000_000..90_000_000).contains(&h.transmit_started_ns)));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn simultaneous_fault_domains_have_order_independent_effective_state() {
        let mut scenario = pat_fixture();
        scenario.faults = vec![
            Fault {
                at_ns: 30_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::SampledPointing,
            },
            Fault {
                at_ns: 30_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::SampledPointing,
            },
        ];
        let first = run(&scenario).unwrap();
        scenario.faults.swap(0, 1);
        scenario.faults.swap(2, 3);
        let second = run(&scenario).unwrap();
        assert_eq!(first.pat_events, second.pat_events);
        assert_eq!(first.control_events, second.control_events);
        assert!(first.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("administrative_link_down")
        }));
        assert_eq!(
            first
                .pat_events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![20_000_000, 90_000_000, 140_000_000]
        );
    }

    #[test]
    fn native_itrs_samples_gate_pat_with_overlapping_admin_faults() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-native-itrs-samples.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 90_000_000, 140_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        let edges: Vec<_> = report
            .control_events
            .iter()
            .filter(|event| {
                event.link_id.as_deref() == Some("isl-a-b")
                    && (event.kind == "link_down" || event.kind == "link_up")
            })
            .map(|event| (event.at_ns, event.kind.as_str()))
            .collect();
        assert_eq!(
            edges,
            vec![(30_000_000, "link_down"), (70_000_000, "link_up")]
        );
        assert!(report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .all(|hop| !(30_000_000..90_000_000).contains(&hop.transmit_started_ns)));
        assert!(report.delivered > 0 && report.dropped > 0);
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn native_itrs_los_rate_gate_forces_pat_reacquisition_without_false_recovery() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-itrs-los-rate-gate.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 90_000_000, 140_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 30_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        let edges: Vec<_> = report
            .control_events
            .iter()
            .filter(|event| {
                event.link_id.as_deref() == Some("isl-a-b")
                    && (event.kind == "link_down" || event.kind == "link_up")
            })
            .map(|event| (event.at_ns, event.kind.as_str()))
            .collect();
        assert_eq!(
            edges,
            vec![(30_000_000, "link_down"), (70_000_000, "link_up")]
        );
        assert!(report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .all(|hop| !(30_000_000..90_000_000).contains(&hop.transmit_started_ns)));
        assert!(report.delivered > 0 && report.dropped > 0);
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
        // Without the opt-in rate gate every sample remains within its
        // steering envelope and tracking is uninterrupted until contact end.
        let mut unbounded = scenario.clone();
        unbounded.optical_tracking_rates.clear();
        let baseline = run(&unbounded).unwrap();
        assert_eq!(
            baseline
                .pat_events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![20_000_000, 90_000_000, 140_000_000]
        );
        assert!(!baseline
            .pat_events
            .iter()
            .any(|event| { event.at_ns == 30_000_000 && event.state == "lost" }));
    }

    #[test]
    fn native_itrs_los_acceleration_gate_releases_pat_and_preserves_fault_domains() {
        let scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-itrs-los-acceleration-gate.json"
        ))
        .unwrap();
        let report = run(&scenario).unwrap();
        let tracking: Vec<_> = report
            .pat_events
            .iter()
            .filter(|event| event.state == "tracking")
            .map(|event| event.at_ns)
            .collect();
        assert_eq!(tracking, vec![20_000_000, 95_000_000, 140_000_000]);
        assert!(report.pat_events.iter().any(|event| {
            event.at_ns == 50_000_000
                && event.state == "lost"
                && event.reason == Some("sampled_pointing_loss")
        }));
        let edges: Vec<_> = report
            .control_events
            .iter()
            .filter(|event| {
                event.link_id.as_deref() == Some("isl-a-b")
                    && (event.kind == "link_down" || event.kind == "link_up")
            })
            .map(|event| (event.at_ns, event.kind.as_str()))
            .collect();
        assert_eq!(
            edges,
            vec![(50_000_000, "link_down"), (75_000_000, "link_up")]
        );
        assert!(report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .all(|hop| !(50_000_000..95_000_000).contains(&hop.transmit_started_ns)));
        assert!(report.delivered > 0 && report.dropped > 0);
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
        let mut unbounded = scenario.clone();
        unbounded.optical_tracking_accelerations.clear();
        let baseline = run(&unbounded).unwrap();
        assert!(!baseline
            .pat_events
            .iter()
            .any(|event| { event.at_ns == 50_000_000 && event.state == "lost" }));
        assert!(baseline.pat_events.iter().any(|event| {
            event.at_ns == 55_000_000
                && event.state == "lost"
                && event.reason == Some("administrative_link_down")
        }));
    }

    #[test]
    fn optical_pointing_envelope_rejects_unsteerable_link_without_forwarding() {
        use ntarlink_contracts::OpticalPointing;
        let mut scenario = pat_fixture();
        scenario.optical_pointing.push(OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 35.0,
            to_off_axis_deg: 10.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 20.0,
        });
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        assert!(!prepared.contacts.iter().any(|c| c.link_id == "isl-a-b"));
        assert_eq!(
            events
                .iter()
                .filter(|e| e.state == "rejected" && e.reason == Some("pointing_outside_envelope"))
                .count(),
            2
        );
        let report = run(&scenario).unwrap();
        assert_eq!(report.delivered, 0);
        assert_eq!(report.injected, report.dropped);
        assert!(!report
            .packets
            .iter()
            .flat_map(|p| &p.hops)
            .any(|h| h.link_id == "isl-a-b"));
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn optical_pointing_envelope_accepts_exact_steering_boundary() {
        use ntarlink_contracts::OpticalPointing;
        let mut scenario = pat_fixture();
        scenario.optical_pointing.push(OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 30.0,
            to_off_axis_deg: 20.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 20.0,
        });
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        let starts: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|c| c.link_id == "isl-a-b")
            .map(|c| c.start_ns)
            .collect();
        assert_eq!(starts, vec![20_000_000, 140_000_000]);
        assert!(!events.iter().any(|e| e.state == "rejected"));
        assert!(run(&scenario).unwrap().delivered > 0);
    }

    #[test]
    fn itrs_contact_snapshots_gate_individual_optical_windows() {
        use ntarlink_contracts::OpticalGeometry;
        let mut scenario = pat_fixture();
        let first = OpticalGeometry {
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
        scenario.optical_geometry.push(first.clone());
        scenario.optical_geometry.push(OpticalGeometry {
            at_ns: 120_000_000,
            from_boresight_itrs: [0.0, 1.0, 0.0],
            ..first
        });
        scenario.validate().unwrap();
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        let windows: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|c| c.link_id == "isl-a-b")
            .map(|c| (c.start_ns, c.end_ns))
            .collect();
        assert_eq!(windows, vec![(20_000_000, 120_000_000)]);
        assert_eq!(
            events
                .iter()
                .filter(|e| e.reason == Some("geometry_outside_envelope"))
                .count(),
            1
        );
        let report = run(&scenario).unwrap();
        assert!(report.delivered > 0 && report.dropped > 0);
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert!(report
            .packets
            .iter()
            .flat_map(|p| &p.hops)
            .filter(|h| h.link_id == "isl-a-b")
            .all(|h| (20_000_000..120_000_000).contains(&h.transmit_started_ns)));
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn optical_head_outage_interrupts_tracking_and_requires_reacquisition() {
        use ntarlink_contracts::OpticalHeadOutage;
        let mut scenario = pat_fixture();
        scenario.optical_head_outages.push(OpticalHeadOutage {
            head_id: "sat-a/head-1".into(),
            start_ns: 50_000_000,
            end_ns: 70_000_000,
        });
        scenario.flows[0].packet_count = 18;
        scenario.flows[0].interval_ns = 10_000_000;
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        let windows: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|contact| contact.link_id == "isl-a-b")
            .map(|contact| (contact.start_ns, contact.end_ns))
            .collect();
        assert_eq!(
            windows,
            vec![
                (20_000_000, 50_000_000),
                (90_000_000, 120_000_000),
                (140_000_000, 200_000_000)
            ]
        );
        prepared.validate().unwrap();
        assert!(events.iter().any(|event| {
            event.at_ns == 50_000_000
                && event.state == "lost"
                && event.reason == Some("optical_head_unavailable")
        }));
        let report = run(&scenario).unwrap();
        assert_eq!(
            report
                .pat_events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![20_000_000, 90_000_000, 140_000_000]
        );
        assert!(report
            .control_events
            .iter()
            .any(|event| { event.at_ns == 95_000_000 && event.kind == "routes_installed" }));
        let hops: Vec<_> = report
            .packets
            .iter()
            .flat_map(|packet| &packet.hops)
            .filter(|hop| hop.link_id == "isl-a-b")
            .collect();
        assert!(!hops.is_empty());
        assert!(hops.iter().all(|hop| windows
            .iter()
            .any(|(start, end)| (*start..*end).contains(&hop.transmit_started_ns))));
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn outage_spanning_first_contact_rejects_initial_acquisition() {
        use ntarlink_contracts::OpticalHeadOutage;
        let mut scenario = pat_fixture();
        scenario.optical_head_outages.push(OpticalHeadOutage {
            head_id: "sat-b/head-1".into(),
            start_ns: 0,
            end_ns: 120_000_000,
        });
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        let optical: Vec<_> = prepared
            .contacts
            .iter()
            .filter(|contact| contact.link_id == "isl-a-b")
            .map(|contact| (contact.start_ns, contact.end_ns))
            .collect();
        assert_eq!(optical, vec![(140_000_000, 200_000_000)]);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![140_000_000]
        );
        assert!(run(&scenario).is_ok());
    }

    #[test]
    fn initially_down_optical_link_must_acquire_after_admin_up() {
        use ntarlink_contracts::Fault;
        let mut scenario = pat_fixture();
        scenario
            .links
            .iter_mut()
            .find(|link| link.id == "isl-a-b")
            .unwrap()
            .up = false;
        scenario.faults.push(Fault {
            at_ns: 50_000_000,
            link_id: "isl-a-b".into(),
            action: LinkAction::Up,
            cause: FaultCause::Administrative,
        });
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        assert!(events
            .iter()
            .any(|event| { event.state == "tracking" && event.at_ns == 70_000_000 }));
        assert_eq!(
            prepared
                .contacts
                .iter()
                .find(|contact| contact.link_id == "isl-a-b")
                .unwrap()
                .start_ns,
            70_000_000
        );
        assert!(run(&scenario).is_ok());
    }

    #[test]
    fn permanently_down_pat_link_reports_gate_fidelity_without_tracking() {
        let mut scenario = pat_fixture();
        scenario
            .links
            .iter_mut()
            .find(|link| link.id == "isl-a-b")
            .unwrap()
            .up = false;
        let report = run(&scenario).unwrap();
        assert_eq!(report.delivered, 0);
        assert_eq!(report.injected, report.dropped);
        assert!(report.pat_events.is_empty());
        assert_eq!(
            report.fidelity,
            "l1_scheduled_contact_pat_gate_packet_model"
        );
    }

    #[test]
    fn redundant_admin_up_does_not_restart_tracking() {
        use ntarlink_contracts::Fault;
        let mut scenario = pat_fixture();
        scenario.faults.push(Fault {
            at_ns: 50_000_000,
            link_id: "isl-a-b".into(),
            action: LinkAction::Up,
            cause: FaultCause::Administrative,
        });
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.state == "tracking")
                .map(|event| event.at_ns)
                .collect::<Vec<_>>(),
            vec![20_000_000, 140_000_000]
        );
        assert_eq!(
            prepared
                .contacts
                .iter()
                .filter(|contact| contact.link_id == "isl-a-b")
                .count(),
            2
        );
    }

    #[test]
    fn optical_head_is_released_for_competing_link_during_fault() {
        use ntarlink_contracts::{Fault, Node, NodeKind, OpticalPatLink};
        let mut scenario = pat_fixture();
        scenario.nodes.push(Node {
            id: "sat-c".into(),
            kind: NodeKind::Satellite,
            latitude_deg: None,
            longitude_deg: None,
        });
        let mut second = scenario
            .links
            .iter()
            .find(|link| link.id == "isl-a-b")
            .unwrap()
            .clone();
        second.id = "isl-a-c".into();
        second.to = "sat-c".into();
        scenario.links.push(second);
        scenario.contacts.push(ntarlink_contracts::ContactWindow {
            link_id: "isl-a-c".into(),
            start_ns: 40_000_000,
            end_ns: 110_000_000,
            propagation_delay_ns: Some(3_500_000),
            propagation_delay_samples: vec![],
        });
        scenario.optical_pat.push(OpticalPatLink {
            link_id: "isl-a-c".into(),
            from_head: "sat-a/head-1".into(),
            to_head: "sat-c/head-1".into(),
            slew_ns: 10_000_000,
            acquisition_ns: 10_000_000,
        });
        scenario.faults = vec![
            Fault {
                at_ns: 50_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 90_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];
        let (prepared, events) = prepare_pat(&scenario).unwrap();
        prepared.validate().unwrap();
        assert!(events.iter().any(|event| {
            event.link_id == "isl-a-c" && event.state == "tracking" && event.at_ns == 70_000_000
        }));
        assert!(!events.iter().any(|event| {
            event.link_id == "isl-a-b"
                && event.state == "tracking"
                && (90_000_000..120_000_000).contains(&event.at_ns)
        }));
        assert!(run(&scenario).is_ok());
    }

    #[test]
    fn pat_fault_splits_sampled_delay_at_both_new_contact_boundaries() {
        use ntarlink_contracts::Fault;
        let mut scenario = pat_fixture();
        scenario.faults = vec![
            Fault {
                at_ns: 50_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Down,
                cause: FaultCause::Administrative,
            },
            Fault {
                at_ns: 70_000_000,
                link_id: "isl-a-b".into(),
                action: LinkAction::Up,
                cause: FaultCause::Administrative,
            },
        ];
        let contact = scenario
            .contacts
            .iter_mut()
            .find(|contact| contact.link_id == "isl-a-b" && contact.start_ns == 0)
            .unwrap();
        contact.propagation_delay_samples = vec![
            PropagationDelaySample {
                at_ns: 0,
                delay_ns: 2_000_000,
            },
            PropagationDelaySample {
                at_ns: 119_999_999,
                delay_ns: 4_000_000,
            },
        ];
        let (prepared, _) = prepare_pat(&scenario).unwrap();
        prepared.validate().unwrap();
        for contact in prepared
            .contacts
            .iter()
            .filter(|contact| contact.link_id == "isl-a-b" && contact.start_ns < 120_000_000)
        {
            let samples = &contact.propagation_delay_samples;
            assert_eq!(samples.first().unwrap().at_ns, contact.start_ns);
            assert_eq!(samples.last().unwrap().at_ns, contact.end_ns - 1);
            assert!(samples.iter().all(|sample| sample.delay_ns > 0));
        }
        assert!(run(&scenario).is_ok());
    }

    #[test]
    fn short_geometric_contact_never_opens_optical_packet_link() {
        let mut scenario = pat_fixture();
        scenario
            .contacts
            .retain(|c| c.link_id != "isl-a-b" || c.start_ns == 0);
        let optical = scenario
            .contacts
            .iter_mut()
            .find(|c| c.link_id == "isl-a-b")
            .unwrap();
        optical.end_ns = 10_000_000;
        let report = run(&scenario).unwrap();
        assert_eq!(report.delivered, 0);
        assert!(report.pat_events.iter().any(|e| e.state == "rejected"));
        assert!(!report
            .control_events
            .iter()
            .any(|e| { e.kind == "contact_open" && e.link_id.as_deref() == Some("isl-a-b") }));
    }

    #[test]
    fn shared_optical_head_conflict_rejects_unacquirable_link() {
        use ntarlink_contracts::{Node, NodeKind, OpticalPatLink};
        let mut scenario = pat_fixture();
        scenario.nodes.push(Node {
            id: "sat-c".into(),
            kind: NodeKind::Satellite,
            latitude_deg: None,
            longitude_deg: None,
        });
        let mut second_link = scenario
            .links
            .iter()
            .find(|l| l.id == "isl-a-b")
            .unwrap()
            .clone();
        second_link.id = "isl-a-c".into();
        second_link.to = "sat-c".into();
        scenario.links.push(second_link);
        scenario.contacts.push(ntarlink_contracts::ContactWindow {
            link_id: "isl-a-c".into(),
            start_ns: 0,
            end_ns: 130_000_000,
            propagation_delay_ns: Some(3_500_000),
            propagation_delay_samples: vec![],
        });
        scenario.optical_pat.push(OpticalPatLink {
            link_id: "isl-a-c".into(),
            from_head: "sat-a/head-1".into(),
            to_head: "sat-c/head-1".into(),
            slew_ns: 10_000_000,
            acquisition_ns: 10_000_000,
        });
        let report = run(&scenario).unwrap();
        assert!(report
            .pat_events
            .iter()
            .any(|e| { e.link_id == "isl-a-c" && e.state == "rejected" }));
        assert!(!report
            .control_events
            .iter()
            .any(|e| { e.link_id.as_deref() == Some("isl-a-c") && e.kind == "contact_open" }));
    }

    #[test]
    fn pat_gate_resamples_geometric_delay_at_tracking_boundary() {
        let mut scenario = pat_fixture();
        let optical = scenario
            .contacts
            .iter_mut()
            .find(|c| c.link_id == "isl-a-b" && c.start_ns == 0)
            .unwrap();
        optical.propagation_delay_samples = vec![
            PropagationDelaySample {
                at_ns: 0,
                delay_ns: 2_000_000,
            },
            PropagationDelaySample {
                at_ns: 119_999_999,
                delay_ns: 4_000_000,
            },
        ];
        let (prepared, _) = prepare_pat(&scenario).unwrap();
        let gated = prepared
            .contacts
            .iter()
            .find(|c| c.link_id == "isl-a-b")
            .unwrap();
        assert_eq!(gated.start_ns, 20_000_000);
        assert_eq!(
            gated.propagation_delay_samples.first().unwrap().at_ns,
            20_000_000
        );
        assert_eq!(
            gated.propagation_delay_samples.last().unwrap().at_ns,
            119_999_999
        );
        prepared.validate().unwrap();
        assert!(run(&scenario).unwrap().delivered > 0);
    }

    #[test]
    fn sampled_delay_interpolates_and_clamps_without_floating_point() {
        let samples = [
            PropagationDelaySample {
                at_ns: 10,
                delay_ns: 10,
            },
            PropagationDelaySample {
                at_ns: 20,
                delay_ns: 31,
            },
            PropagationDelaySample {
                at_ns: 30,
                delay_ns: 11,
            },
        ];
        assert_eq!(interpolated_delay(&samples, 0), 10);
        assert_eq!(interpolated_delay(&samples, 15), 21);
        assert_eq!(interpolated_delay(&samples, 25), 21);
        assert_eq!(interpolated_delay(&samples, 35), 11);
    }

    #[test]
    fn packet_propagation_uses_transmission_completion_time() {
        let mut scenario = basic();
        let end_ns = scenario.duration_ns;
        scenario.contacts.push(ntarlink_contracts::ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 0,
            end_ns,
            propagation_delay_ns: Some(2_000_000),
            propagation_delay_samples: vec![
                PropagationDelaySample {
                    at_ns: 0,
                    delay_ns: 1_000_000,
                },
                PropagationDelaySample {
                    at_ns: end_ns - 1,
                    delay_ns: 3_000_000,
                },
            ],
        });
        let result = run(&scenario).unwrap();
        assert_eq!(result.delivered, 3);
        let samples = &scenario.contacts[0].propagation_delay_samples;
        let user_hops: Vec<_> = result
            .packets
            .iter()
            .map(|packet| {
                packet
                    .hops
                    .iter()
                    .find(|hop| hop.link_id == "user-ue-a")
                    .unwrap()
            })
            .collect();
        assert!(user_hops
            .windows(2)
            .all(|pair| pair[0].propagation_ns < pair[1].propagation_ns));
        for hop in user_hops {
            assert_eq!(
                hop.propagation_ns,
                interpolated_delay(samples, hop.transmit_completed_ns)
            );
            assert_eq!(
                hop.arrived_ns - hop.transmit_completed_ns,
                hop.propagation_ns + hop.processing_ns
            );
        }
        assert_eq!(
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap(),
            serde_json::to_vec(&result).unwrap()
        );
    }

    #[test]
    fn same_scenario_is_byte_for_byte_deterministic() {
        let a = serde_json::to_vec(&run(&basic()).unwrap()).unwrap();
        let b = serde_json::to_vec(&run(&basic()).unwrap()).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn finite_queue_drops_packets_with_reason() {
        let mut s = basic();
        s.flows[0].packet_count = 4;
        s.flows[0].interval_ns = 1;
        s.links[0].queue_limit_bytes = s.flows[0].packet_size_bytes;
        let result = run(&s).unwrap();
        assert!(result.dropped > 0);
        assert!(result
            .packets
            .iter()
            .any(|p| p.drop_reason.as_deref() == Some("queue_overflow")));
    }

    #[test]
    fn fault_reconverges_over_backup_isl() {
        let s = Scenario::from_json(include_str!("../../../scenarios/isl-failure.json")).unwrap();
        let r = run(&s).unwrap();
        assert!(r.control_events.iter().any(|e| e.kind == "link_down"));
        assert!(r.route_revision >= 2);
        assert!(r
            .packets
            .iter()
            .any(|p| p.hops.iter().any(|h| h.link_id == "isl-a-c")));
        assert!(r
            .packets
            .iter()
            .any(|p| p.status == PacketStatus::Delivered));
    }

    #[test]
    fn geometric_contacts_drive_handover_without_teleportation() {
        let scenario =
            Scenario::from_json(include_str!("../../../scenarios/visibility-handover.json"))
                .unwrap();
        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, 12);
        assert!(report.delivered > 0);
        assert!(report.dropped > 0);
        assert!(report.route_revision >= 3);
        assert!(report
            .control_events
            .iter()
            .any(|e| e.kind == "contact_close"));
        assert!(report
            .control_events
            .iter()
            .any(|e| e.kind == "contact_open"));
        assert!(report
            .packets
            .iter()
            .any(|p| p.hops.iter().any(|h| h.link_id == "user-ue-c")));
        assert!(report.packets.iter().any(|p| p
            .hops
            .iter()
            .any(|h| { h.link_id == "user-ue-a" && h.propagation_ns == 1_800_000 })));
        assert!(report
            .packets
            .iter()
            .all(|p| p.status != PacketStatus::InFlight));
    }

    #[test]
    fn scheduled_contact_mode_does_not_allow_unscheduled_rf_or_feeder_links() {
        let mut scenario = basic();
        scenario.contact_mode = ntarlink_contracts::ContactMode::Scheduled;
        let report = run(&scenario).unwrap();
        assert_eq!(report.delivered, 0);
        assert_eq!(report.dropped, report.injected);
        assert_eq!(report.fidelity, "l1_sampled_orbital_contact_packet_model");
    }

    #[test]
    fn a_link_remains_down_when_faulted_during_visible_contact() {
        let mut scenario =
            Scenario::from_json(include_str!("../../../scenarios/visibility-handover.json"))
                .unwrap();
        scenario.faults.push(ntarlink_contracts::Fault {
            at_ns: 20_000_000,
            link_id: "user-ue-a".into(),
            action: ntarlink_contracts::LinkAction::Down,
            cause: FaultCause::Administrative,
        });
        let result = run(&scenario).unwrap();
        assert!(result.dropped > 0);
        assert!(result.control_events.iter().any(|e| e.kind == "link_down"));
        assert!(result
            .packets
            .iter()
            .any(|p| p.hops.iter().any(|h| h.link_id == "user-ue-c")));
    }

    fn two_terminal_beam() -> Scenario {
        use ntarlink_contracts::{AccessBeam, BeamBearer, Flow, Link, Node, NodeKind};
        let mut scenario = basic();
        scenario.nodes.push(Node {
            id: "ue-b".into(),
            kind: NodeKind::Terminal,
            latitude_deg: None,
            longitude_deg: None,
        });
        scenario.links.push(Link {
            id: "user-ue-b".into(),
            from: "ue-b".into(),
            to: "sat-a".into(),
            kind: LinkKind::UserRf,
            capacity_bps: 100_000_000,
            propagation_delay_ns: 2_000_000,
            processing_delay_ns: 300_000,
            queue_limit_bytes: 4800,
            up: true,
        });
        scenario.flows = vec![
            Flow {
                id: "a".into(),
                source: "ue-a".into(),
                destination: "internet".into(),
                packet_size_bytes: 1200,
                packet_count: 4,
                start_ns: 0,
                interval_ns: 1,
            },
            Flow {
                id: "b".into(),
                source: "ue-b".into(),
                destination: "internet".into(),
                packet_size_bytes: 1200,
                packet_count: 4,
                start_ns: 0,
                interval_ns: 1,
            },
        ];
        scenario.access_beams.push(AccessBeam {
            id: "beam-sat-a-uplink".into(),
            capacity_bps: 976_000,
            frame_duration_ns: 10_000_000,
            quantum_bytes: 1220,
            packet_overhead_bytes: 20,
            queue_limit_bytes: 20_000,
            bearers: vec![
                BeamBearer {
                    terminal_id: "ue-a".into(),
                    link_id: "user-ue-a".into(),
                    weight: 1,
                },
                BeamBearer {
                    terminal_id: "ue-b".into(),
                    link_id: "user-ue-b".into(),
                    weight: 1,
                },
            ],
        });
        scenario
    }

    #[test]
    fn shared_beam_grants_bound_two_terminal_packet_throughput() {
        let scenario = two_terminal_beam();
        let report = run(&scenario).unwrap();
        assert_eq!(
            (report.injected, report.delivered, report.dropped),
            (8, 8, 0)
        );
        assert_eq!(report.beam_grants.len(), 8);
        assert!(report
            .beam_grants
            .iter()
            .enumerate()
            .all(|(i, g)| g.frame_start_ns == i as u64 * 10_000_000
                && g.wire_bytes == 1220
                && g.transmit_completed_ns - g.transmit_started_ns == 10_000_000));
        let mut hops: Vec<_> = report
            .packets
            .iter()
            .map(|p| {
                p.hops
                    .iter()
                    .find(|h| h.link_id.starts_with("user-"))
                    .unwrap()
            })
            .collect();
        hops.sort_by_key(|h| h.transmit_started_ns);
        assert!(hops
            .windows(2)
            .all(|w| w[1].transmit_started_ns - w[0].transmit_started_ns >= 10_000_000));
        assert_eq!(hops.last().unwrap().transmit_started_ns, 70_000_000);
        assert!(hops.iter().any(|h| h.queue_delay_ns > 0));
        assert_eq!(
            serde_json::to_vec(&report).unwrap(),
            serde_json::to_vec(&run(&scenario).unwrap()).unwrap()
        );
    }

    #[test]
    fn contact_close_cancels_only_ungranted_bearer_packets() {
        let mut scenario = two_terminal_beam();
        scenario.contacts.push(ntarlink_contracts::ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 0,
            end_ns: 25_000_000,
            propagation_delay_ns: Some(2_000_000),
            propagation_delay_samples: vec![],
        });
        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert!(report
            .packets
            .iter()
            .any(|p| p.drop_reason.as_deref() == Some("beam_link_lost")));
        assert!(report
            .packets
            .iter()
            .filter(|p| p.flow_id == "b")
            .all(|p| p.status == PacketStatus::Delivered));
        assert!(report
            .packets
            .iter()
            .filter(|p| p.flow_id == "a")
            .any(|p| p.status == PacketStatus::Delivered));
    }

    #[test]
    fn shared_beam_queue_limit_drops_excess_without_leaking_packets() {
        let mut scenario = two_terminal_beam();
        scenario.access_beams[0].queue_limit_bytes = 2440;
        let report = run(&scenario).unwrap();
        assert_eq!(report.injected, report.delivered + report.dropped);
        assert!(report
            .packets
            .iter()
            .any(|p| p.drop_reason.as_deref() == Some("beam_queue_overflow")));
        assert!(report.delivered > 0);
    }

    #[test]
    fn disconnected_feeder_is_not_teleported() {
        let mut s = basic();
        let feeder = s.links.iter_mut().find(|l| l.id == "feeder-b-gw").unwrap();
        feeder.up = false;
        let result = run(&s).unwrap();
        assert_eq!(result.delivered, 0);
        assert!(result
            .packets
            .iter()
            .all(|p| p.drop_reason.as_deref() == Some("no_route")));
    }
}
