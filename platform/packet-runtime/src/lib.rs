use ntarlink_access_scheduler::{BeamConfig, BeamScheduler, TerminalSpec};
use ntarlink_contracts::{ContactMode, LinkAction, LinkKind, PropagationDelaySample, Scenario};
use ntarlink_network_control::next_hops;
use ntarlink_simulation::Scheduler;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};

const MAX_PROCESSED_EVENTS: u64 = 5_000_000;

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

/// Execute an L1 fixed-contact, modeled-packet simulation.
/// This does not model RF waveform, optical pointing or a proprietary modem.
pub fn run(scenario: &Scenario) -> Result<SimulationReport, String> {
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
        assert!(report.beam_grants.iter().enumerate().all(|(i, g)|
            g.frame_start_ns == i as u64 * 10_000_000 && g.wire_bytes == 1220
                && g.transmit_completed_ns - g.transmit_started_ns == 10_000_000
        ));
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
