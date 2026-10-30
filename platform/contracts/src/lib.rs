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

/// Provenance for a scheduled link transition, not a physical measurement.
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FaultCause {
    #[default]
    Administrative,
    SampledPointing,
}

impl FaultCause {
    fn is_administrative(&self) -> bool {
        *self == Self::Administrative
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fault {
    pub at_ns: u64,
    pub link_id: String,
    pub action: LinkAction,
    /// Legacy faults default to administrative; omit this field on serialization.
    #[serde(default, skip_serializing_if = "FaultCause::is_administrative")]
    pub cause: FaultCause,
}

/// A sampled one-way geometric propagation delay in scenario-relative nanoseconds.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PropagationDelaySample {
    pub at_ns: u64,
    pub delay_ns: u64,
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
    /// Optional per-transmission delay samples; interpolated at TX completion.
    /// The midpoint delay above remains the route-planning weight.
    #[serde(default)]
    pub propagation_delay_samples: Vec<PropagationDelaySample>,
}

/// A terminal's directional uplink radio bearer on a shared beam.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BeamBearer {
    pub terminal_id: String,
    pub link_id: String,
    pub weight: u32,
}

/// Opt-in finite shared MAC resource for multiple user RF uplinks.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccessBeam {
    pub id: String,
    pub capacity_bps: u64,
    pub frame_duration_ns: u64,
    pub quantum_bytes: u64,
    pub packet_overhead_bytes: u64,
    pub queue_limit_bytes: u64,
    pub bearers: Vec<BeamBearer>,
}

/// Opt-in deterministic PAT acquisition assumptions for a directed optical ISL.
/// Optical head identifiers are exclusive resources owned by their endpoint satellite.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalPatLink {
    pub link_id: String,
    pub from_head: String,
    pub to_head: String,
    pub slew_ns: u64,
    pub acquisition_ns: u64,
}

/// SYS-02 scheduled unavailability of one spacecraft-owned optical head.
/// The half-open interval is in scenario-relative virtual nanoseconds.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalHeadOutage {
    pub head_id: String,
    pub start_ns: u64,
    pub end_ns: u64,
}

/// SYS-02 operator-supplied static off-boresight angles and per-head steering limits.
/// Degrees are fixed over every contact; no attitude or target vector is inferred.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalPointing {
    pub link_id: String,
    pub from_off_axis_deg: f64,
    pub to_off_axis_deg: f64,
    pub from_max_off_axis_deg: f64,
    pub to_max_off_axis_deg: f64,
}

/// A discrete SYS-02 steering observation evaluated by the Rust PAT planner.
/// Angles are degrees; the value is held until the next sample on that link.
/// These are supplied observations, not a continuous attitude/ephemeris solution.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalSteeringSample {
    pub link_id: String,
    pub at_ns: u64,
    pub from_off_axis_deg: f64,
    pub to_off_axis_deg: f64,
    pub from_max_off_axis_deg: f64,
    pub to_max_off_axis_deg: f64,
}

/// One frame-tagged SYS-02 pointing snapshot at the start of a geometric ISL contact.
/// All vectors are ITRS Earth-fixed Cartesian coordinates; positions are meters.
/// Boresight directions are unitless and fixed over the entire contact window.
/// These are supplied snapshots, not inferred spacecraft attitude or a live ephemeris.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OpticalGeometry {
    pub link_id: String,
    pub at_ns: u64,
    pub frame: String,
    pub from_position_m: [f64; 3],
    pub to_position_m: [f64; 3],
    pub from_boresight_itrs: [f64; 3],
    pub to_boresight_itrs: [f64; 3],
    pub from_max_off_axis_deg: f64,
    pub to_max_off_axis_deg: f64,
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
    /// Optional uplink beam-frame MAC; links not listed retain legacy packet transport.
    #[serde(default)]
    pub access_beams: Vec<AccessBeam>,
    /// Opt-in PAT gating for geometric optical contacts; absent means legacy behavior.
    #[serde(default)]
    pub optical_pat: Vec<OpticalPatLink>,
    /// Optional SYS-02 head maintenance/failure windows; only PAT heads are supported.
    #[serde(default)]
    pub optical_head_outages: Vec<OpticalHeadOutage>,
    /// Optional all-PAT-link static pointing envelope assumptions (degrees).
    #[serde(default)]
    pub optical_pointing: Vec<OpticalPointing>,
    /// Optional runtime-evaluated piecewise-constant steering samples for all PAT links.
    #[serde(default)]
    pub optical_steering_samples: Vec<OpticalSteeringSample>,
    /// Optional time-indexed ITRS positions and boresights evaluated by Rust PAT.
    /// Each observation is held until the next sample on the same link.
    #[serde(default)]
    pub optical_geometry_samples: Vec<OpticalGeometry>,
    /// Optional ITRS position/boresight snapshots at each PAT contact start.
    #[serde(default)]
    pub optical_geometry: Vec<OpticalGeometry>,
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
        if self.access_beams.len() > 10_000 {
            return Err("access beam limit exceeded".into());
        }
        let mut beam_ids = HashSet::new();
        let mut bearer_link_ids = HashSet::new();
        for beam in &self.access_beams {
            if beam.id.trim().is_empty() || !beam_ids.insert(&beam.id) {
                return Err("access beam IDs must be nonempty and unique".into());
            }
            if beam.capacity_bps == 0
                || beam.frame_duration_ns == 0
                || beam.frame_duration_ns > self.duration_ns
                || beam.quantum_bytes == 0
                || beam.queue_limit_bytes == 0
                || beam.bearers.is_empty()
            {
                return Err(format!("invalid access beam configuration: {}", beam.id));
            }
            let frame_bytes = u128::from(beam.capacity_bps) * u128::from(beam.frame_duration_ns)
                / 8_000_000_000u128;
            if frame_bytes == 0
                || frame_bytes > u128::from(u64::MAX)
                || frame_bytes <= u128::from(beam.packet_overhead_bytes)
            {
                return Err(format!("invalid access beam frame capacity: {}", beam.id));
            }
            let mut terminal_ids = HashSet::new();
            let mut satellite: Option<&str> = None;
            for bearer in &beam.bearers {
                if bearer.weight == 0
                    || bearer.terminal_id.trim().is_empty()
                    || !terminal_ids.insert(&bearer.terminal_id)
                    || !bearer_link_ids.insert(&bearer.link_id)
                {
                    return Err(format!(
                        "invalid or duplicate access bearer: {}",
                        bearer.link_id
                    ));
                }
                let link = self
                    .links
                    .iter()
                    .find(|l| l.id == bearer.link_id)
                    .ok_or_else(|| format!("unknown access bearer link: {}", bearer.link_id))?;
                if link.kind != LinkKind::UserRf
                    || link.from != bearer.terminal_id
                    || find_kind(&link.from) != Some(&NodeKind::Terminal)
                    || find_kind(&link.to) != Some(&NodeKind::Satellite)
                {
                    return Err(format!(
                        "access bearer must be a terminal uplink: {}",
                        link.id
                    ));
                }
                if satellite.is_some_and(|id| id != link.to) {
                    return Err(format!(
                        "access beam spans multiple satellites: {}",
                        beam.id
                    ));
                }
                satellite = Some(&link.to);
            }
        }
        for fault in &self.faults {
            if fault.at_ns >= self.duration_ns || !link_ids.contains(&fault.link_id) {
                return Err(format!("invalid fault target or time: {}", fault.link_id));
            }
            if fault.cause == FaultCause::SampledPointing
                && !self
                    .optical_pat
                    .iter()
                    .any(|pat| pat.link_id == fault.link_id)
            {
                return Err(format!(
                    "sampled pointing fault requires an optical PAT link: {}",
                    fault.link_id
                ));
            }
        }
        if self.contacts.len() > 500_000 {
            return Err("contact window limit exceeded".into());
        }
        let mut windows: std::collections::HashMap<&str, Vec<(u64, u64)>> =
            std::collections::HashMap::new();
        let mut sample_total = 0usize;
        for contact in &self.contacts {
            let Some(link) = self.links.iter().find(|l| l.id == contact.link_id) else {
                return Err(format!("unknown contact link: {}", contact.link_id));
            };
            if !matches!(
                link.kind,
                LinkKind::UserRf | LinkKind::Feeder | LinkKind::OpticalIsl
            ) {
                return Err(format!(
                    "contact window not supported for terrestrial link: {}",
                    contact.link_id
                ));
            }
            if contact.start_ns >= contact.end_ns || contact.end_ns > self.duration_ns {
                return Err(format!("invalid contact interval for {}", contact.link_id));
            }
            if contact.propagation_delay_ns == Some(0) {
                return Err(format!(
                    "contact propagation delay must be positive for {}",
                    contact.link_id
                ));
            }
            let samples = &contact.propagation_delay_samples;
            sample_total = sample_total
                .checked_add(samples.len())
                .ok_or("propagation delay sample count overflow")?;
            if sample_total > 500_000 {
                return Err("propagation delay sample budget exceeded".into());
            }
            if !samples.is_empty() {
                if contact.propagation_delay_ns.is_none() {
                    return Err(format!(
                        "sampled contact requires midpoint propagation delay for {}",
                        contact.link_id
                    ));
                }
                if samples.first().map(|s| s.at_ns) != Some(contact.start_ns)
                    || samples.last().map(|s| s.at_ns) != Some(contact.end_ns - 1)
                {
                    return Err(format!(
                        "propagation delay samples must span the contact for {}",
                        contact.link_id
                    ));
                }
                let mut previous = None;
                for sample in samples {
                    if sample.delay_ns == 0
                        || sample.at_ns < contact.start_ns
                        || sample.at_ns >= contact.end_ns
                        || previous.is_some_and(|at| sample.at_ns <= at)
                    {
                        return Err(format!(
                            "invalid propagation delay sample for {}",
                            contact.link_id
                        ));
                    }
                    previous = Some(sample.at_ns);
                }
            }
            windows
                .entry(&contact.link_id)
                .or_default()
                .push((contact.start_ns, contact.end_ns));
        }
        for (link_id, intervals) in &mut windows {
            intervals.sort_unstable();
            for pair in intervals.windows(2) {
                if pair[0].1 > pair[1].0 {
                    return Err(format!("overlapping contact intervals for {link_id}"));
                }
            }
        }
        if self.optical_pat.len() > 10_000 {
            return Err("optical PAT link limit exceeded".into());
        }
        if !self.optical_pat.is_empty() && self.contact_mode != ContactMode::Scheduled {
            return Err("optical PAT requires scheduled contact mode".into());
        }
        let mut pat_links = HashSet::new();
        let mut head_owners: std::collections::HashMap<&str, &str> =
            std::collections::HashMap::new();
        for pat in &self.optical_pat {
            if pat.link_id.trim().is_empty()
                || pat.from_head.trim().is_empty()
                || pat.to_head.trim().is_empty()
                || pat.from_head == pat.to_head
                || pat.acquisition_ns == 0
                || pat.slew_ns.checked_add(pat.acquisition_ns).is_none()
                || !pat_links.insert(&pat.link_id)
            {
                return Err(format!(
                    "invalid or duplicate optical PAT link: {}",
                    pat.link_id
                ));
            }
            let link = self
                .links
                .iter()
                .find(|link| link.id == pat.link_id)
                .ok_or_else(|| format!("unknown optical PAT link: {}", pat.link_id))?;
            if link.kind != LinkKind::OpticalIsl || !windows.contains_key(link.id.as_str()) {
                return Err(format!(
                    "optical PAT requires geometric ISL contacts: {}",
                    link.id
                ));
            }
            for (head, satellite) in [(&pat.from_head, &link.from), (&pat.to_head, &link.to)] {
                if let Some(owner) = head_owners.insert(head.as_str(), satellite.as_str()) {
                    if owner != satellite {
                        return Err(format!("optical head shared across satellites: {head}"));
                    }
                }
            }
        }
        if self.optical_head_outages.len() > 10_000 {
            return Err("optical head outage limit exceeded".into());
        }
        let mut outages: std::collections::HashMap<&str, Vec<(u64, u64)>> =
            std::collections::HashMap::new();
        for outage in &self.optical_head_outages {
            if !head_owners.contains_key(outage.head_id.as_str()) {
                return Err(format!("unknown PAT optical head: {}", outage.head_id));
            }
            if outage.start_ns >= outage.end_ns || outage.end_ns > self.duration_ns {
                return Err(format!("invalid optical head outage: {}", outage.head_id));
            }
            outages
                .entry(outage.head_id.as_str())
                .or_default()
                .push((outage.start_ns, outage.end_ns));
        }
        for (head, intervals) in &mut outages {
            intervals.sort_unstable();
            if intervals.windows(2).any(|pair| pair[0].1 > pair[1].0) {
                return Err(format!("overlapping optical head outages: {head}"));
            }
        }
        if self.optical_pointing.len() > 10_000 {
            return Err("optical pointing limit exceeded".into());
        }
        if !self.optical_pointing.is_empty()
            && self.optical_pointing.len() != self.optical_pat.len()
        {
            return Err("optical pointing must cover every PAT link".into());
        }
        let mut pointing_links = HashSet::new();
        let mut head_limits: std::collections::HashMap<&str, f64> =
            std::collections::HashMap::new();
        for pointing in &self.optical_pointing {
            if !pointing_links.insert(&pointing.link_id) {
                return Err(format!(
                    "duplicate optical pointing link: {}",
                    pointing.link_id
                ));
            }
            let pat = self
                .optical_pat
                .iter()
                .find(|pat| pat.link_id == pointing.link_id)
                .ok_or_else(|| format!("unknown PAT pointing link: {}", pointing.link_id))?;
            for (head, angle, limit) in [
                (
                    &pat.from_head,
                    pointing.from_off_axis_deg,
                    pointing.from_max_off_axis_deg,
                ),
                (
                    &pat.to_head,
                    pointing.to_off_axis_deg,
                    pointing.to_max_off_axis_deg,
                ),
            ] {
                if !angle.is_finite()
                    || !limit.is_finite()
                    || !(0.0..=180.0).contains(&angle)
                    || !(0.0..=180.0).contains(&limit)
                {
                    return Err(format!("invalid optical pointing angle for {head}"));
                }
                if head_limits
                    .insert(head, limit)
                    .is_some_and(|previous| previous != limit)
                {
                    return Err(format!("inconsistent optical head pointing limit: {head}"));
                }
            }
        }
        if self.optical_steering_samples.len() > 10_000 {
            return Err("optical steering sample budget exceeded".into());
        }
        if !self.optical_steering_samples.is_empty() {
            if !self.optical_pointing.is_empty()
                || !self.optical_geometry.is_empty()
                || !self.optical_geometry_samples.is_empty()
            {
                return Err(
                    "optical steering samples cannot mix with other pointing modes".into(),
                );
            }
            let mut last_by_link: std::collections::HashMap<&str, u64> =
                std::collections::HashMap::new();
            let mut sample_limits: std::collections::HashMap<&str, f64> =
                std::collections::HashMap::new();
            for sample in &self.optical_steering_samples {
                let pat = self
                    .optical_pat
                    .iter()
                    .find(|pat| pat.link_id == sample.link_id)
                    .ok_or_else(|| {
                        format!("unknown PAT steering sample link: {}", sample.link_id)
                    })?;
                if sample.at_ns >= self.duration_ns {
                    return Err(format!(
                        "optical steering sample out of horizon: {}",
                        sample.link_id
                    ));
                }
                if let Some(previous) = last_by_link.insert(&sample.link_id, sample.at_ns) {
                    if sample.at_ns <= previous {
                        return Err(format!(
                            "optical steering samples must be strictly increasing: {}",
                            sample.link_id
                        ));
                    }
                } else if sample.at_ns != 0 {
                    return Err(format!(
                        "optical steering samples must start at zero: {}",
                        sample.link_id
                    ));
                }
                for (head, angle, limit) in [
                    (
                        pat.from_head.as_str(),
                        sample.from_off_axis_deg,
                        sample.from_max_off_axis_deg,
                    ),
                    (
                        pat.to_head.as_str(),
                        sample.to_off_axis_deg,
                        sample.to_max_off_axis_deg,
                    ),
                ] {
                    if !angle.is_finite()
                        || !limit.is_finite()
                        || !(0.0..=180.0).contains(&angle)
                        || !(0.0..=180.0).contains(&limit)
                    {
                        return Err(format!("invalid optical steering sample angle: {head}"));
                    }
                    if sample_limits
                        .insert(head, limit)
                        .is_some_and(|previous| previous != limit)
                    {
                        return Err(format!("inconsistent sampled optical head limit: {head}"));
                    }
                }
            }
            if last_by_link.len() != pat_links.len() {
                return Err("optical steering samples must cover every PAT link".into());
            }
            if self
                .faults
                .iter()
                .any(|fault| fault.cause == FaultCause::SampledPointing)
            {
                return Err(
                    "sampled pointing faults cannot mix with native optical steering samples"
                        .into(),
                );
            }
        }
        if self.optical_geometry.len() > 10_000 {
            return Err("optical geometry snapshot limit exceeded".into());
        }
        if !self.optical_geometry.is_empty() && !self.optical_pointing.is_empty() {
            return Err("optical geometry and static pointing are mutually exclusive".into());
        }
        if !self.optical_geometry.is_empty() {
            let pat_contacts: HashSet<(&str, u64)> = self
                .contacts
                .iter()
                .filter(|contact| pat_links.contains(&contact.link_id))
                .map(|contact| (contact.link_id.as_str(), contact.start_ns))
                .collect();
            if self.optical_geometry.len() != pat_contacts.len() {
                return Err("optical geometry must cover every PAT contact".into());
            }
            let mut seen = HashSet::new();
            let mut limits: std::collections::HashMap<&str, f64> = std::collections::HashMap::new();
            for geometry in &self.optical_geometry {
                let key = (geometry.link_id.as_str(), geometry.at_ns);
                if !pat_contacts.contains(&key) || !seen.insert(key) {
                    return Err(format!(
                        "unknown or duplicate optical geometry contact: {} at {}",
                        geometry.link_id, geometry.at_ns
                    ));
                }
                if geometry.frame != "ITRS" {
                    return Err(format!(
                        "optical geometry requires ITRS frame: {}",
                        geometry.link_id
                    ));
                }
                let pat = self
                    .optical_pat
                    .iter()
                    .find(|pat| pat.link_id == geometry.link_id)
                    .expect("validated optical geometry PAT link");
                for vector in [
                    geometry.from_position_m,
                    geometry.to_position_m,
                    geometry.from_boresight_itrs,
                    geometry.to_boresight_itrs,
                ] {
                    if vector.iter().any(|v| !v.is_finite() || v.abs() > 1.0e12) {
                        return Err(format!(
                            "invalid optical geometry vector: {}",
                            geometry.link_id
                        ));
                    }
                }
                for boresight in [geometry.from_boresight_itrs, geometry.to_boresight_itrs] {
                    if boresight.iter().all(|v| *v == 0.0) {
                        return Err(format!("zero optical boresight: {}", geometry.link_id));
                    }
                }
                if geometry.from_position_m == geometry.to_position_m {
                    return Err(format!(
                        "coincident optical positions: {}",
                        geometry.link_id
                    ));
                }
                for (head, limit) in [
                    (pat.from_head.as_str(), geometry.from_max_off_axis_deg),
                    (pat.to_head.as_str(), geometry.to_max_off_axis_deg),
                ] {
                    if !limit.is_finite() || !(0.0..=180.0).contains(&limit) {
                        return Err(format!("invalid optical geometry steering limit: {head}"));
                    }
                    if limits
                        .insert(head, limit)
                        .is_some_and(|previous| previous != limit)
                    {
                        return Err(format!("inconsistent optical geometry head limit: {head}"));
                    }
                }
            }
        }
        if self.optical_geometry_samples.len() > 10_000 {
            return Err("optical ITRS sample budget exceeded".into());
        }
        if !self.optical_geometry_samples.is_empty() {
            if !self.optical_geometry.is_empty()
                || !self.optical_pointing.is_empty()
                || !self.optical_steering_samples.is_empty()
            {
                return Err("optical ITRS samples cannot mix with other pointing modes".into());
            }
            let mut last_by_link: std::collections::HashMap<&str, u64> =
                std::collections::HashMap::new();
            let mut limits: std::collections::HashMap<&str, f64> =
                std::collections::HashMap::new();
            for sample in &self.optical_geometry_samples {
                let pat = self
                    .optical_pat
                    .iter()
                    .find(|pat| pat.link_id == sample.link_id)
                    .ok_or_else(|| format!("unknown PAT ITRS sample link: {}", sample.link_id))?;
                if sample.at_ns >= self.duration_ns {
                    return Err(format!(
                        "optical ITRS sample out of horizon: {}",
                        sample.link_id
                    ));
                }
                if let Some(previous) = last_by_link.insert(&sample.link_id, sample.at_ns) {
                    if sample.at_ns <= previous {
                        return Err(format!(
                            "optical ITRS samples must be strictly increasing: {}",
                            sample.link_id
                        ));
                    }
                } else if sample.at_ns != 0 {
                    return Err(format!(
                        "optical ITRS samples must start at zero: {}",
                        sample.link_id
                    ));
                }
                if sample.frame != "ITRS" {
                    return Err(format!(
                        "optical ITRS sample requires ITRS frame: {}",
                        sample.link_id
                    ));
                }
                for vector in [
                    sample.from_position_m,
                    sample.to_position_m,
                    sample.from_boresight_itrs,
                    sample.to_boresight_itrs,
                ] {
                    if vector
                        .iter()
                        .any(|value| !value.is_finite() || value.abs() > 1.0e12)
                    {
                        return Err(format!(
                            "invalid optical ITRS sample vector: {}",
                            sample.link_id
                        ));
                    }
                }
                for boresight in [sample.from_boresight_itrs, sample.to_boresight_itrs] {
                    if boresight.iter().all(|value| *value == 0.0) {
                        return Err(format!(
                            "zero optical ITRS sample boresight: {}",
                            sample.link_id
                        ));
                    }
                }
                if sample.from_position_m == sample.to_position_m {
                    return Err(format!(
                        "coincident optical ITRS sample positions: {}",
                        sample.link_id
                    ));
                }
                for (head, limit) in [
                    (pat.from_head.as_str(), sample.from_max_off_axis_deg),
                    (pat.to_head.as_str(), sample.to_max_off_axis_deg),
                ] {
                    if !limit.is_finite() || !(0.0..=180.0).contains(&limit) {
                        return Err(format!("invalid optical ITRS sample steering limit: {head}"));
                    }
                    if limits
                        .insert(head, limit)
                        .is_some_and(|previous| previous != limit)
                    {
                        return Err(format!("inconsistent optical ITRS sample head limit: {head}"));
                    }
                }
            }
            if last_by_link.len() != pat_links.len() {
                return Err("optical ITRS samples must cover every PAT link".into());
            }
            if self
                .faults
                .iter()
                .any(|fault| fault.cause == FaultCause::SampledPointing)
            {
                return Err("sampled pointing faults cannot mix with optical ITRS samples".into());
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
    fn optical_pat_requires_scheduled_geometric_isl_and_valid_heads() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.optical_pat.push(OpticalPatLink {
            link_id: "isl-a-b".into(),
            from_head: "sat-a/head-1".into(),
            to_head: "sat-b/head-1".into(),
            slew_ns: 10,
            acquisition_ns: 20,
        });
        assert!(s.validate().unwrap_err().contains("scheduled"));
        s.contact_mode = ContactMode::Scheduled;
        assert!(s.validate().unwrap_err().contains("geometric"));
        s.contacts.push(ContactWindow {
            link_id: "isl-a-b".into(),
            start_ns: 0,
            end_ns: 100,
            propagation_delay_ns: None,
            propagation_delay_samples: vec![],
        });
        s.validate().unwrap();
        s.optical_pat[0].acquisition_ns = 0;
        assert!(s.validate().is_err());
        s.optical_pat[0].acquisition_ns = 20;
        s.optical_pat.push(s.optical_pat[0].clone());
        assert!(s.validate().unwrap_err().contains("duplicate"));
    }

    #[test]
    fn optical_head_outages_validate_ownership_bounds_and_overlap() {
        let mut s = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        s.optical_head_outages.push(OpticalHeadOutage {
            head_id: "sat-a/head-1".into(),
            start_ns: 50_000_000,
            end_ns: 70_000_000,
        });
        s.validate().unwrap();
        s.optical_head_outages.push(OpticalHeadOutage {
            head_id: "sat-a/head-1".into(),
            start_ns: 70_000_000,
            end_ns: 80_000_000,
        });
        s.validate().unwrap();
        s.optical_head_outages[1].start_ns = 69_000_000;
        assert!(s.validate().unwrap_err().contains("overlapping"));
        s.optical_head_outages.pop();
        s.optical_head_outages[0].head_id = "sat-c/head-1".into();
        assert!(s.validate().unwrap_err().contains("unknown PAT"));
        s.optical_head_outages[0].head_id = "sat-a/head-1".into();
        s.optical_head_outages[0].end_ns = s.duration_ns + 1;
        assert!(s
            .validate()
            .unwrap_err()
            .contains("invalid optical head outage"));
        s.optical_head_outages[0].end_ns = s.optical_head_outages[0].start_ns;
        assert!(s.validate().is_err());
    }

    #[test]
    fn optical_pointing_requires_complete_finite_pat_envelopes() {
        let mut s = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        s.optical_pointing.push(OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 30.0,
            to_off_axis_deg: 20.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 20.0,
        });
        s.validate().unwrap();
        s.optical_pointing[0].to_off_axis_deg = f64::NAN;
        assert!(s
            .validate()
            .unwrap_err()
            .contains("invalid optical pointing"));
        s.optical_pointing[0].to_off_axis_deg = 20.0;
        s.optical_pointing[0].from_max_off_axis_deg = -1.0;
        assert!(s.validate().is_err());
        s.optical_pointing[0].from_max_off_axis_deg = 30.0;
        s.optical_pointing[0].link_id = "missing".into();
        assert!(s.validate().unwrap_err().contains("unknown PAT"));
        s.optical_pointing[0].link_id = "isl-a-b".into();
        s.optical_pointing.push(s.optical_pointing[0].clone());
        assert!(s.validate().is_err());
        s.optical_pointing.clear();
        s.validate().unwrap();
    }

    #[test]
    fn optical_geometry_requires_complete_itrs_snapshots() {
        let mut s = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        let sample = OpticalGeometry {
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
        s.optical_geometry.push(sample.clone());
        assert!(s
            .validate()
            .unwrap_err()
            .contains("cover every PAT contact"));
        s.optical_geometry.push(OpticalGeometry {
            at_ns: 120_000_000,
            ..sample.clone()
        });
        s.validate().unwrap();
        s.optical_geometry[1].frame = "TEME".into();
        assert!(s.validate().unwrap_err().contains("ITRS"));
        s.optical_geometry[1].frame = "ITRS".into();
        s.optical_geometry[1].to_boresight_itrs = [0.0; 3];
        assert!(s.validate().unwrap_err().contains("zero optical boresight"));
        s.optical_geometry[1].to_boresight_itrs = [-1.0, 0.0, 0.0];
        s.optical_geometry[1].from_position_m = [f64::NAN, 0.0, 0.0];
        assert!(s
            .validate()
            .unwrap_err()
            .contains("invalid optical geometry"));
        s.optical_geometry[1].from_position_m = sample.from_position_m;
        s.optical_geometry[1].at_ns = 0;
        assert!(s
            .validate()
            .unwrap_err()
            .contains("duplicate optical geometry"));
        s.optical_geometry[1].at_ns = 120_000_000;
        s.optical_pointing.push(OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 0.0,
            to_off_axis_deg: 0.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        });
        assert!(s.validate().unwrap_err().contains("mutually exclusive"));
    }

    #[test]
    fn native_itrs_samples_validate_frame_timeline_and_fault_exclusivity() {
        let mut s = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        let sample = OpticalGeometry {
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
        s.optical_geometry_samples.push(sample.clone());
        s.validate().unwrap();
        s.optical_geometry_samples.push(OpticalGeometry {
            at_ns: 30_000_000,
            from_boresight_itrs: [0.0, 1.0, 0.0],
            ..sample.clone()
        });
        s.validate().unwrap();
        s.optical_geometry_samples[1].at_ns = 0;
        assert!(s.validate().unwrap_err().contains("strictly increasing"));
        s.optical_geometry_samples[1].at_ns = 30_000_000;
        s.optical_geometry_samples[0].at_ns = 1;
        assert!(s.validate().unwrap_err().contains("start at zero"));
        s.optical_geometry_samples[0].at_ns = 0;
        s.optical_geometry_samples[1].frame = "TEME".into();
        assert!(s.validate().unwrap_err().contains("ITRS frame"));
        s.optical_geometry_samples[1].frame = "ITRS".into();
        s.optical_geometry_samples[1].from_position_m[0] = f64::NAN;
        assert!(s.validate().unwrap_err().contains("invalid optical ITRS"));
        s.optical_geometry_samples[1].from_position_m = sample.from_position_m;
        s.optical_geometry_samples[1].to_boresight_itrs = [0.0; 3];
        assert!(s.validate().unwrap_err().contains("zero optical ITRS"));
        s.optical_geometry_samples[1].to_boresight_itrs = sample.to_boresight_itrs;
        s.optical_geometry_samples[1].to_max_off_axis_deg = 20.0;
        assert!(s.validate().unwrap_err().contains("inconsistent optical ITRS"));
        s.optical_geometry_samples[1].to_max_off_axis_deg = 30.0;
        s.faults.push(Fault {
            at_ns: 50_000_000,
            link_id: "isl-a-b".into(),
            action: LinkAction::Down,
            cause: FaultCause::SampledPointing,
        });
        assert!(s.validate().unwrap_err().contains("cannot mix"));
        s.faults.clear();
        s.optical_pointing.push(OpticalPointing {
            link_id: "isl-a-b".into(),
            from_off_axis_deg: 0.0,
            to_off_axis_deg: 0.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        });
        assert!(s.validate().unwrap_err().contains("cannot mix"));
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
            propagation_delay_samples: vec![],
        });
        assert!(s.validate().is_ok());
        s.contacts.push(ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 19,
            end_ns: 22,
            propagation_delay_ns: None,
            propagation_delay_samples: vec![],
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
            propagation_delay_samples: vec![],
        });
        assert!(s.validate().unwrap_err().contains("unknown contact"));
        s.contacts[0].link_id = "isl-a-b".into();
        s.contacts[0].end_ns = 10;
        assert!(s.validate().unwrap_err().contains("invalid contact"));
    }

    #[test]
    fn sampled_delays_require_bounded_ordered_positive_samples() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.contacts.push(ContactWindow {
            link_id: "user-ue-a".into(),
            start_ns: 10,
            end_ns: 20,
            propagation_delay_ns: Some(1_900_000),
            propagation_delay_samples: vec![
                PropagationDelaySample {
                    at_ns: 10,
                    delay_ns: 1_000_000,
                },
                PropagationDelaySample {
                    at_ns: 19,
                    delay_ns: 2_000_000,
                },
            ],
        });
        assert!(s.validate().is_ok());
        s.contacts[0].propagation_delay_samples[1].at_ns = 10;
        assert!(s.validate().unwrap_err().contains("span the contact"));
        s.contacts[0].propagation_delay_samples[1].at_ns = 19;
        s.contacts[0].propagation_delay_samples[1].delay_ns = 0;
        assert!(s
            .validate()
            .unwrap_err()
            .contains("invalid propagation delay sample"));
        s.contacts[0].propagation_delay_samples[1].delay_ns = 2_000_000;
        s.contacts[0].propagation_delay_ns = None;
        assert!(s.validate().unwrap_err().contains("requires midpoint"));
    }

    #[test]
    fn access_beams_validate_direction_capacity_and_shared_resources() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        let beam = AccessBeam {
            id: "beam-a".into(),
            capacity_bps: 80_000,
            frame_duration_ns: 10_000_000,
            quantum_bytes: 100,
            packet_overhead_bytes: 20,
            queue_limit_bytes: 10_000,
            bearers: vec![BeamBearer {
                terminal_id: "ue-a".into(),
                link_id: "user-ue-a".into(),
                weight: 1,
            }],
        };
        s.access_beams.push(beam);
        assert!(s.validate().is_ok());
        s.access_beams[0].bearers[0].link_id = "feeder-b-gw".into();
        assert!(s.validate().unwrap_err().contains("terminal uplink"));
        s.access_beams[0].bearers[0].link_id = "user-ue-a".into();
        s.access_beams.push(s.access_beams[0].clone());
        assert!(s.validate().unwrap_err().contains("beam IDs"));
        s.access_beams[1].id = "beam-b".into();
        assert!(s
            .validate()
            .unwrap_err()
            .contains("duplicate access bearer"));
        s.access_beams.pop();
        s.access_beams[0].capacity_bps = 1;
        assert!(s.validate().unwrap_err().contains("frame capacity"));
    }

    #[test]
    fn sampled_pointing_fault_cause_is_typed_and_backward_compatible() {
        let legacy = r#"{"at_ns":10,"link_id":"isl-a-b","action":"down"}"#;
        let fault: Fault = serde_json::from_str(legacy).unwrap();
        assert_eq!(fault.cause, FaultCause::Administrative);
        assert_eq!(serde_json::to_string(&fault).unwrap(), legacy);
        let sampled =
            r#"{"at_ns":10,"link_id":"isl-a-b","action":"down","cause":"sampled_pointing"}"#;
        let fault: Fault = serde_json::from_str(sampled).unwrap();
        assert_eq!(fault.cause, FaultCause::SampledPointing);
        assert_eq!(serde_json::to_string(&fault).unwrap(), sampled);
        assert!(serde_json::from_str::<Fault>(
            r#"{"at_ns":10,"link_id":"isl-a-b","action":"down","cause":"unknown"}"#
        )
        .is_err());

        let mut scenario = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        scenario.faults.push(fault);
        scenario.validate().unwrap();
        scenario.faults[0].link_id = "user-ue-a".into();
        assert!(scenario
            .validate()
            .unwrap_err()
            .contains("requires an optical PAT link"));
    }

    #[test]
    fn native_optical_steering_samples_validate_timeline_and_exclusivity() {
        let mut s = Scenario::from_json(include_str!(
            "../../../scenarios/optical-pat-acquisition.json"
        ))
        .unwrap();
        let sample = OpticalSteeringSample {
            link_id: "isl-a-b".into(),
            at_ns: 0,
            from_off_axis_deg: 10.0,
            to_off_axis_deg: 10.0,
            from_max_off_axis_deg: 30.0,
            to_max_off_axis_deg: 30.0,
        };
        s.optical_steering_samples.push(sample.clone());
        s.validate().unwrap();
        s.optical_steering_samples.push(OpticalSteeringSample {
            at_ns: 30_000_000,
            from_off_axis_deg: 40.0,
            ..sample.clone()
        });
        s.validate().unwrap();
        s.optical_steering_samples[1].at_ns = 0;
        assert!(s.validate().unwrap_err().contains("strictly increasing"));
        s.optical_steering_samples[1].at_ns = 30_000_000;
        s.optical_steering_samples[0].at_ns = 1;
        assert!(s.validate().unwrap_err().contains("start at zero"));
        s.optical_steering_samples[0].at_ns = 0;
        s.optical_steering_samples[1].from_max_off_axis_deg = 20.0;
        assert!(s.validate().unwrap_err().contains("inconsistent sampled"));
        s.optical_steering_samples[1].from_max_off_axis_deg = 30.0;
        s.optical_steering_samples[1].to_off_axis_deg = f64::NAN;
        assert!(s
            .validate()
            .unwrap_err()
            .contains("invalid optical steering"));
        s.optical_steering_samples[1].to_off_axis_deg = 10.0;
        s.faults.push(Fault {
            at_ns: 50_000_000,
            link_id: "isl-a-b".into(),
            action: LinkAction::Down,
            cause: FaultCause::SampledPointing,
        });
        assert!(s.validate().unwrap_err().contains("cannot mix"));
    }

    #[test]
    fn rejects_non_finite_coordinates() {
        let mut s = Scenario::from_json(&fixture()).unwrap();
        s.nodes[0].latitude_deg = Some(f64::NAN);
        assert!(s.validate().is_err());
    }
}
