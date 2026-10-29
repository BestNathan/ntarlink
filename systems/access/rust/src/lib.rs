//! SYS-05 deterministic, finite-capacity beam-frame scheduler.
//!
//! This is a modeled MAC resource allocator, not a PHY waveform or a
//! connection to the Rust packet runtime. Grants represent one directional
//! frame; they do not establish a bearer or transmit a packet.

use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BeamConfig {
    pub capacity_bps: u64,
    pub frame_duration_ns: u64,
    pub quantum_bytes: u64,
    pub packet_overhead_bytes: u64,
    /// Aggregate wire-byte queue limit shared by all terminals.
    pub queue_limit_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerminalSpec {
    pub id: String,
    pub weight: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Grant {
    pub terminal_id: String,
    pub packet_id: String,
    pub payload_bytes: u64,
    pub wire_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameReport {
    pub frame_index: u64,
    pub capacity_bytes: u64,
    pub used_bytes: u64,
    pub queued_bytes: u64,
    pub queued_packets: usize,
    pub grants: Vec<Grant>,
}

#[derive(Clone, Debug)]
struct Packet {
    id: String,
    payload_bytes: u64,
    wire_bytes: u64,
}

#[derive(Clone, Debug)]
struct Terminal {
    id: String,
    weight: u32,
    deficit_bytes: u64,
    pending: VecDeque<Packet>,
}

/// Weighted deficit round-robin over deterministic terminal-ID order.
#[derive(Clone, Debug)]
pub struct BeamScheduler {
    config: BeamConfig,
    capacity_bytes: u64,
    terminals: Vec<Terminal>,
    queued_bytes: u64,
    next_frame: u64,
    cursor: usize,
}

impl BeamScheduler {
    pub fn new(config: BeamConfig, specs: Vec<TerminalSpec>) -> Result<Self, String> {
        if config.capacity_bps == 0
            || config.frame_duration_ns == 0
            || config.quantum_bytes == 0
            || config.queue_limit_bytes == 0
        {
            return Err("capacity, duration, quantum and queue limit must be positive".into());
        }
        // All intermediate products fit in u128 for u64 inputs.
        let capacity_bytes = u64::try_from(
            u128::from(config.capacity_bps) * u128::from(config.frame_duration_ns)
                / 8_000_000_000u128,
        )
        .map_err(|_| "frame byte budget exceeds u64".to_owned())?;
        if capacity_bytes == 0 {
            return Err("frame cannot carry one wire byte".into());
        }
        if specs.is_empty() {
            return Err("beam requires at least one terminal".into());
        }
        let mut seen = HashSet::new();
        let mut terminals = Vec::with_capacity(specs.len());
        for spec in specs {
            if spec.id.trim().is_empty() || !seen.insert(spec.id.clone()) {
                return Err("terminal IDs must be nonempty and unique".into());
            }
            if spec.weight == 0 {
                return Err("terminal weight must be positive".into());
            }
            terminals.push(Terminal {
                id: spec.id,
                weight: spec.weight,
                deficit_bytes: 0,
                pending: VecDeque::new(),
            });
        }
        terminals.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(Self {
            config,
            capacity_bytes,
            terminals,
            queued_bytes: 0,
            next_frame: 0,
            cursor: 0,
        })
    }

    pub fn frame_capacity_bytes(&self) -> u64 {
        self.capacity_bytes
    }

    pub fn queued_bytes(&self) -> u64 {
        self.queued_bytes
    }

    pub fn queued_packets(&self) -> usize {
        self.terminals.iter().map(|t| t.pending.len()).sum()
    }

    /// Enqueue atomically; no packet fragmentation or implicit queue eviction.
    pub fn enqueue(
        &mut self,
        terminal_id: &str,
        packet_id: impl Into<String>,
        payload_bytes: u64,
    ) -> Result<(), String> {
        let index = self
            .terminals
            .binary_search_by(|t| t.id.as_str().cmp(terminal_id))
            .map_err(|_| format!("unknown terminal: {terminal_id}"))?;
        if payload_bytes == 0 {
            return Err("packet payload must be positive".into());
        }
        let wire_bytes = payload_bytes
            .checked_add(self.config.packet_overhead_bytes)
            .ok_or("packet wire size overflow")?;
        if wire_bytes > self.capacity_bytes {
            return Err("packet cannot fit in one frame".into());
        }
        if wire_bytes
            > self
                .config
                .queue_limit_bytes
                .saturating_sub(self.queued_bytes)
        {
            return Err("beam queue is full".into());
        }
        let packet_id = packet_id.into();
        if packet_id.trim().is_empty() {
            return Err("packet ID must be nonempty".into());
        }
        self.terminals[index].pending.push_back(Packet {
            id: packet_id,
            payload_bytes,
            wire_bytes,
        });
        self.queued_bytes += wire_bytes;
        Ok(())
    }

    /// Drop all queued packets for a bearer when contact or link availability is lost.
    /// Already-issued grants are not revoked (P0 completion-after-departure).
    pub fn discard_terminal(&mut self, terminal_id: &str) -> Result<Vec<String>, String> {
        let index = self.terminals.binary_search_by(|t| t.id.as_str().cmp(terminal_id))
            .map_err(|_| format!("unknown terminal: {terminal_id}"))?;
        let mut removed = Vec::new();
        let terminal = &mut self.terminals[index];
        while let Some(packet) = terminal.pending.pop_front() {
            self.queued_bytes -= packet.wire_bytes;
            removed.push(packet.id);
        }
        terminal.deficit_bytes = 0;
        Ok(removed)
    }

    /// Grant one frame. Each backlogged terminal earns weight * quantum bytes
    /// once per frame, capped at one frame. Idle terminals lose old credit.
    /// A rotating starting terminal avoids fixed-order tight-budget bias.
    pub fn schedule_frame(&mut self) -> Result<FrameReport, String> {
        let next_frame = self
            .next_frame
            .checked_add(1)
            .ok_or("frame index overflow")?;
        for terminal in &mut self.terminals {
            if terminal.pending.is_empty() {
                terminal.deficit_bytes = 0;
            } else {
                let earned = u128::from(terminal.weight) * u128::from(self.config.quantum_bytes);
                terminal.deficit_bytes = (u128::from(terminal.deficit_bytes) + earned)
                    .min(u128::from(self.capacity_bytes))
                    as u64;
            }
        }
        let mut grants = Vec::new();
        let mut used_bytes = 0u64;
        let len = self.terminals.len();
        for offset in 0..len {
            let index = (self.cursor + offset) % len;
            let terminal = &mut self.terminals[index];
            while let Some(packet) = terminal.pending.front() {
                if packet.wire_bytes > terminal.deficit_bytes
                    || packet.wire_bytes > self.capacity_bytes - used_bytes
                {
                    break;
                }
                let packet = terminal.pending.pop_front().expect("front exists");
                terminal.deficit_bytes -= packet.wire_bytes;
                self.queued_bytes -= packet.wire_bytes;
                used_bytes += packet.wire_bytes;
                grants.push(Grant {
                    terminal_id: terminal.id.clone(),
                    packet_id: packet.id,
                    payload_bytes: packet.payload_bytes,
                    wire_bytes: packet.wire_bytes,
                });
            }
            if terminal.pending.is_empty() {
                terminal.deficit_bytes = 0;
            }
        }
        self.cursor = (self.cursor + 1) % len;
        self.next_frame = next_frame;
        Ok(FrameReport {
            frame_index: next_frame - 1,
            capacity_bytes: self.capacity_bytes,
            used_bytes,
            queued_bytes: self.queued_bytes,
            queued_packets: self.queued_packets(),
            grants,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(frame_bytes: u64) -> BeamConfig {
        BeamConfig {
            capacity_bps: frame_bytes * 8,
            frame_duration_ns: 1_000_000_000,
            quantum_bytes: 100,
            packet_overhead_bytes: 20,
            queue_limit_bytes: 100_000,
        }
    }

    fn terminals() -> Vec<TerminalSpec> {
        vec![
            TerminalSpec {
                id: "ue-b".into(),
                weight: 1,
            },
            TerminalSpec {
                id: "ue-a".into(),
                weight: 3,
            },
        ]
    }

    #[test]
    fn saturated_two_terminal_traffic_is_weighted_and_capacity_bounded() {
        let mut beam = BeamScheduler::new(config(400), terminals()).unwrap();
        for i in 0..60 {
            beam.enqueue("ue-a", format!("a-{i}"), 80).unwrap();
            beam.enqueue("ue-b", format!("b-{i}"), 80).unwrap();
        }
        let mut a = 0;
        let mut b = 0;
        for index in 0..10 {
            let report = beam.schedule_frame().unwrap();
            assert_eq!(report.frame_index, index);
            assert_eq!(report.capacity_bytes, 400);
            assert_eq!(report.used_bytes, 400);
            assert_eq!(report.grants.len(), 4);
            a += report
                .grants
                .iter()
                .filter(|g| g.terminal_id == "ue-a")
                .count();
            b += report
                .grants
                .iter()
                .filter(|g| g.terminal_id == "ue-b")
                .count();
        }
        assert_eq!((a, b), (30, 10));
        assert_eq!(beam.queued_packets(), 80);
        assert_eq!(beam.queued_bytes(), 8000);
    }

    #[test]
    fn tight_frame_budget_rotates_service_between_terminals() {
        let mut beam = BeamScheduler::new(config(100), terminals()).unwrap();
        for i in 0..4 {
            beam.enqueue("ue-a", format!("a-{i}"), 80).unwrap();
            beam.enqueue("ue-b", format!("b-{i}"), 80).unwrap();
        }
        let served: Vec<_> = (0..8)
            .map(|_| {
                let report = beam.schedule_frame().unwrap();
                assert_eq!(report.used_bytes, 100);
                report.grants[0].terminal_id.clone()
            })
            .collect();
        assert_eq!(
            served,
            vec!["ue-a", "ue-b", "ue-a", "ue-b", "ue-a", "ue-b", "ue-a", "ue-b"]
        );
        assert_eq!(beam.queued_bytes(), 0);
    }

    #[test]
    fn large_packet_accrues_credit_and_is_eventually_granted() {
        let mut beam = BeamScheduler::new(
            config(500),
            vec![TerminalSpec {
                id: "ue".into(),
                weight: 1,
            }],
        )
        .unwrap();
        beam.enqueue("ue", "large", 380).unwrap();
        for _ in 0..3 {
            assert!(beam.schedule_frame().unwrap().grants.is_empty());
        }
        let report = beam.schedule_frame().unwrap();
        assert_eq!(report.used_bytes, 400);
        assert_eq!(report.grants[0].wire_bytes, 400);
        assert_eq!(report.queued_bytes, 0);
    }

    #[test]
    fn invalid_configuration_and_terminals_are_rejected() {
        let mut invalid = config(400);
        invalid.quantum_bytes = 0;
        assert!(BeamScheduler::new(invalid, terminals()).is_err());
        assert!(BeamScheduler::new(config(400), vec![]).is_err());
        assert!(BeamScheduler::new(
            config(400),
            vec![TerminalSpec {
                id: "x".into(),
                weight: 0
            }]
        )
        .is_err());
        assert!(BeamScheduler::new(
            config(400),
            vec![
                TerminalSpec {
                    id: "x".into(),
                    weight: 1
                },
                TerminalSpec {
                    id: "x".into(),
                    weight: 1
                },
            ]
        )
        .is_err());
        assert!(BeamScheduler::new(
            BeamConfig {
                capacity_bps: 1,
                frame_duration_ns: 1,
                ..config(400)
            },
            terminals()
        )
        .is_err());
        assert!(BeamScheduler::new(
            BeamConfig {
                capacity_bps: u64::MAX,
                frame_duration_ns: u64::MAX,
                ..config(400)
            },
            terminals()
        )
        .is_err());
    }

    #[test]
    fn packet_validation_and_queue_overflow_do_not_mutate_state() {
        let mut config = config(200);
        config.queue_limit_bytes = 200;
        let mut beam = BeamScheduler::new(config, terminals()).unwrap();
        for (terminal, id, size) in [
            ("missing", "p", 80),
            ("ue-a", "p", 0),
            ("ue-a", "p", 181),
            ("ue-a", "p", u64::MAX),
            ("ue-a", "", 80),
        ] {
            assert!(beam.enqueue(terminal, id, size).is_err());
        }
        assert_eq!(beam.queued_bytes(), 0);
        beam.enqueue("ue-a", "first", 80).unwrap();
        beam.enqueue("ue-b", "second", 80).unwrap();
        assert!(beam.enqueue("ue-a", "third", 80).is_err());
        assert_eq!(beam.queued_bytes(), 200);
        assert_eq!(beam.schedule_frame().unwrap().used_bytes, 200);
        assert_eq!(beam.queued_packets(), 0);
    }

    #[test]
    fn discarding_one_bearer_preserves_other_terminal_and_queue_accounting() {
        let mut beam = BeamScheduler::new(config(400), terminals()).unwrap();
        beam.enqueue("ue-a", "a", 80).unwrap();
        beam.enqueue("ue-b", "b", 80).unwrap();
        assert_eq!(beam.discard_terminal("ue-a").unwrap(), vec!["a"]);
        assert_eq!(beam.queued_bytes(), 100);
        assert_eq!(beam.queued_packets(), 1);
        assert!(beam.discard_terminal("missing").is_err());
        let grants = beam.schedule_frame().unwrap().grants;
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].packet_id, "b");
    }

    #[test]
    fn input_order_does_not_change_deterministic_grants() {
        let mut a = BeamScheduler::new(config(400), terminals()).unwrap();
        let mut b =
            BeamScheduler::new(config(400), terminals().into_iter().rev().collect()).unwrap();
        for beam in [&mut a, &mut b] {
            assert!(beam.schedule_frame().unwrap().grants.is_empty());
            beam.enqueue("ue-b", "b", 80).unwrap();
            beam.enqueue("ue-a", "a", 80).unwrap();
        }
        assert_eq!(a.schedule_frame().unwrap(), b.schedule_frame().unwrap());
    }
}
