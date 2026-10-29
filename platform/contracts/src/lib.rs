use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Terminal,
    Satellite,
    GroundStation,
    Pop,
    Internet,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: String,
    pub kind: NodeKind,
    #[serde(default)]
    pub latitude_deg: Option<f64>,
    #[serde(default)]
    pub longitude_deg: Option<f64>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LinkKind {
    UserRf,
    OpticalIsl,
    Feeder,
    Backhaul,
    Internet,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Link {
    pub id: String,
    pub from: String,
    pub to: String,
    pub kind: LinkKind,
    pub capacity_bps: u64,
    pub propagation_delay_ns: u64,
    #[serde(default)]
    pub processing_delay_ns: u64,
    pub queue_limit_bytes: u64,
    #[serde(default = "default_true")]
    pub up: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub id: String,
    pub source: String,
    pub destination: String,
    pub packet_size_bytes: u64,
    pub packet_count: u32,
    pub start_ns: u64,
    pub interval_ns: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LinkAction {
    Up,
    Down,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub at_ns: u64,
    pub link_id: String,
    pub action: LinkAction,
}

/// A half-open geometric link contact. This is not a claim of PHY acquisition.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ContactWindow {
    pub link_id: String,
    pub start_ns: u64,
    pub end_ns: u64,
    /// Optional effective geometric propagation delay, sampled at a declared epoch.
    /// The active window uses this fixed value until the next contact transition.
    #[serde(default)]
    pub propagation_delay_ns: Option<u64>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContactMode {
    #[default]
    Static,
    Scheduled,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    pub schema_version: u32,
    pub name: String,
    pub duration_ns: u64,
    pub control_delay_ns: u64,
    #[serde(default)]
    pub contact_mode: ContactMode,
    pub nodes: Vec<Node>,
    pub links: Vec<Link>,
    pub flows: Vec<Flow>,
    #[serde(default)]
    pub faults: Vec<Fault>,
    /// Optional physical visibility/contact schedule. Empty means static-link mode.
    #[serde(default)]
    pub contacts: Vec<ContactWindow>,
}

impl Scenario {
    pub fn from_json(content: &str) -> Result<Self, String> {
        let scenario: Self = serde_json::from_str(content).map_err(|e| e.to_string())?;
        scenario.validate()?;
        Ok(scenario)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("only scenario schema_version 1 is supported".into());
        }
        if self.name.trim().is_empty() || self.duration_ns == 0 {
            return Err("scenario name and positive duration_ns are required".into());
        }
        if self.nodes.len() > 100_000 || self.links.len() > 500_000 || self.flows.len() > 100_000 {
            return Err("scenario exceeds configured entity limits".into());
        }
        if self.nodes.is_empty() || self.links.is_empty() || self.flows.is_empty() {
            return Err("scenario needs nodes, links and flows".into());
        }
        let mut ids = HashSet::new();
        for node in &self.nodes {
            if node.id.trim().is_empty() || !ids.insert(&node.id) {
                return Err(format!("empty or duplicate node ID: {}", node.id));
            }
            if let Some(lat) = node.latitude_deg {
                if !lat.is_finite() || !(-90.0..=90.0).contains(&lat) {
                    return Err(format!("invalid latitude for {}", node.id));
                }
            }
            if let Some(lon) = node.longitude_deg {
                if !lon.is_finite() || !(-180.0..=180.0).contains(&lon) {
                    return Err(format!("invalid longitude for {}", node.id));
                }
            }
        }
        let find_kind = |id: &str| self.nodes.iter().find(|n| n.id == id).map(|n| &n.kind);
        let mut link_ids = HashSet::new();
        for link in &self.links {
            if link.id.trim().is_empty() || !link_ids.insert(&link.id) {
                return Err(format!("empty or duplicate link ID: {}", link.id));
            }
            let a = find_kind(&link.from).ok_or_else(|| format!("unknown node: {}", link.from))?;
            let b = find_kind(&link.to).ok_or_else(|| format!("unknown node: {}", link.to))?;
            if link.from == link.to {
                return Err(format!("self link not supported: {}", link.id));
            }
            let allowed = match link.kind {
                LinkKind::UserRf => matches!(
                    (a, b),
                    (NodeKind::Terminal, NodeKind::Satellite)
                        | (NodeKind::Satellite, NodeKind::Terminal)
                ),
                LinkKind::OpticalIsl => {
                    matches!((a, b), (NodeKind::Satellite, NodeKind::Satellite))
                }
                LinkKind::Feeder => matches!(
                    (a, b),
                    (NodeKind::Satellite, NodeKind::GroundStation)
                        | (NodeKind::GroundStation, NodeKind::Satellite)
                ),
                LinkKind::Backhaul => matches!(
                    (a, b),
                    (NodeKind::GroundStation, NodeKind::Pop)
                        | (NodeKind::Pop, NodeKind::GroundStation)
                ),
                LinkKind::Internet => matches!(
                    (a, b),
                    (NodeKind::Pop, NodeKind::Internet) | (NodeKind::Internet, NodeKind::Pop)
                ),
            };
            if !allowed {
                return Err(format!("invalid endpoints for link {}", link.id));
            }
            if link.capacity_bps == 0 || link.queue_limit_bytes == 0 {
                return Err(format!("nonpositive capacity/queue for {}", link.id));
            }
        }
        let mut flow_ids = HashSet::new();
        let mut packet_total = 0u64;
        for flow in &self.flows {
            if flow.id.trim().is_empty() || !flow_ids.insert(&flow.id) {
                return Err(format!("empty or duplicate flow ID: {}", flow.id));
            }
            if find_kind(&flow.source) != Some(&NodeKind::Terminal)
                || find_kind(&flow.destination) != Some(&NodeKind::Internet)
            {
                return Err(format!("flow {} requires terminal to internet", flow.id));
            }
            if flow.packet_count == 0 || !(1..=65_535).contains(&flow.packet_size_bytes) {
                return Err(format!("invalid packet size/count in flow {}", flow.id));
            }
            if flow.packet_count > 1 && flow.interval_ns == 0 {
                return Err(format!("flow {} requires nonzero interval", flow.id));
            }
            let span = u64::from(flow.packet_count - 1)
                .checked_mul(flow.interval_ns)
                .and_then(|n| flow.start_ns.checked_add(n))
                .ok_or_else(|| format!("time overflow in flow {}", flow.id))?;
            if span >= self.duration_ns {
                return Err(format!("flow {} exceeds duration", flow.id));
            }
            packet_total += u64::from(flow.packet_count);
            if packet_total > 1_000_000 {
                return Err("packet limit exceeded (1,000,000)".into());
            }
        }
        for fault in &self.faults {
            if fault.at_ns >= self.duration_ns || !link_ids.contains(&fault.link_id) {
                return Err(format!("invalid fault target or time: {}", fault.link_id));
            }
        }
        if self.contacts.len() > 500_000 {
            return Err("contact window limit exceeded".into());
        }
        let mut windows: std::collections::HashMap<&str, Vec<(u64, u64)>> =
            std::collections::HashMap::new();
        for contact in &self.contacts {
            let Some(link) = self.links.iter().find(|l| l.id == contact.link_id) else {
                return Err(format!("unknown contact link: {}", contact.link_id));
            };
            if !matches!(link.kind, LinkKind::UserRf | LinkKind::Feeder | LinkKind::OpticalIsl) {
                return Err(format!("contact window not supported for terrestrial link: {}", contact.link_id));
            }
            if contact.start_ns >= contact.end_ns || contact.end_ns > self.duration_ns {
                return Err(format!("invalid contact interval for {}", contact.link_id));
            }
            if contact.propagation_delay_ns == Some(0) {
                return Err(format!("contact propagation delay must be positive for {}", contact.link_id));
            }
            windows.entry(&contact.link_id).or_default().push((contact.start_ns, contact.end_ns));
        }
        for (link_id, intervals) in &mut windows {
            intervals.sort_unstable();
            for pair in intervals.windows(2) {
                if pair[0].1 > pair[1].0 {
                    return Err(format!("overlapping contact intervals for {link_id}"));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> String {
        include_str!("../../../scenarios/minimal.json").to_string()
    }

    #[test]
    fn minimal_validates() {
        Scenario::from_json(&fixture()).unwrap();
    }

    #[test]
    fn duplicate_node_rejected() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.nodes.push(s.nodes[0].clone());
        assert!(s.validate().unwrap_err().contains("duplicate node"));
    }

    #[test]
    fn wrong_link_endpoints_rejected() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.links[0].kind = LinkKind::OpticalIsl;
        assert!(s.validate().unwrap_err().contains("invalid endpoints"));
    }

    #[test]
    fn unknown_node_and_invalid_packet_count_rejected() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.links[0].to = "missing".into();
        assert!(s.validate().unwrap_err().contains("unknown node"));
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.flows[0].packet_count = 0;
        assert!(s.validate().is_err());
    }

    #[test]
    fn contact_windows_validate_bounds_and_overlaps() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.contacts.push(ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 10,
            end_ns: 20,
            propagation_delay_ns: Some(1_900_000),
        });
        assert!(s.validate().is_ok());
        s.contacts.push(ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 19,
            end_ns: 22,
            propagation_delay_ns: None,
        });
        assert!(s.validate().unwrap_err().contains("overlapping"));
        s.contacts[1].start_ns = 20;
        assert!(s.validate().is_ok());
        s.contacts[1].link_id = "pop-internet".into();
        assert!(s.validate().unwrap_err().contains("terrestrial"));
    }

    #[test]
    fn contact_windows_reject_unknown_link_and_invalid_range() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.contacts.push(ContactWindow {
            link_id: "not-a-link".into(),
            start_ns: 10,
            end_ns: 20,
            propagation_delay_ns: Some(2_000),
        });
        assert!(s.validate().unwrap_err().contains("unknown contact"));
        s.contacts[0].link_id = "isl-a-b".into();
        s.contacts[0].end_ns = 10;
        assert!(s.validate().unwrap_err().contains("invalid contact"));
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.nodes[0].latitude_deg = Some(f64::NAN);
        assert!(s.validate().is_err());
    }
}
