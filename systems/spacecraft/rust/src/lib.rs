//! SYS-02 deterministic optical-head availability, not physical pointing.
use ntarlink_contracts::{
    OpticalBodyTrackingSample, OpticalGeometry, OpticalHeadOutage, OpticalHeadSlewActuator,
    OpticalPointing,
};
use std::collections::{HashMap, HashSet};

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

/// Single owner of sampled predicted BODY-frame head state and immutable
/// historical snapshots. PAT consumes projections; it does not mutate this
/// command timeline or own the follower's actuator state.
#[derive(Debug)]
pub struct BodyHeadRuntime {
    heads: HashMap<String, BodyHeadFollower>,
    history: HashMap<String, Vec<(u64, BodyHeadFollower)>>,
    /// Link-scoped command leases. PAT scheduling still owns its own reservations.
    owners: HashMap<String, String>,
    link_heads: HashMap<String, (String, String)>,
    /// Virtual-time acquisition boundary for each active link lease.
    lease_start_ns: HashMap<String, u64>,
    /// Earliest allowed time for a new command after a timed lease release.
    release_floor_ns: HashMap<String, u64>,
}

/// Lease-only PAT command ownership state, initialized from the actual
/// SYS-02 actuator registry. It audits a canonical virtual-time timeline
/// without replaying or modifying the provisional follower's motion history.
/// This is not a live shared-head command scheduler.
#[derive(Debug, Default)]
pub struct BodyHeadLeaseAudit {
    known_heads: HashSet<String>,
    owners: HashMap<String, String>,
    link_heads: HashMap<String, (String, String)>,
    last_success_ns: HashMap<String, u64>,
    last_transition: HashMap<String, (u64, LeaseAuditPhase)>,
}

/// Same-time transitions must follow the canonical PAT order. Failed sampled
/// commands still consume their position in virtual time, but never advance
/// the successful actuator-command history.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum LeaseAuditPhase {
    Release,
    Reserve,
    Sample,
}

impl BodyHeadLeaseAudit {
    pub fn contains_head(&self, head_id: &str) -> bool {
        self.known_heads.contains(head_id)
    }

    fn check_transition(
        &self,
        from: &str,
        to: &str,
        at_ns: u64,
        phase: LeaseAuditPhase,
    ) -> Result<(), String> {
        if [from, to].into_iter().any(|head| {
            self.last_transition
                .get(head)
                .is_some_and(|previous| *previous > (at_ns, phase))
        }) {
            return Err("PAT head lease audit transition order mismatch".into());
        }
        Ok(())
    }

    fn record_transition(&mut self, from: &str, to: &str, at_ns: u64, phase: LeaseAuditPhase) {
        self.last_transition
            .insert(from.to_owned(), (at_ns, phase));
        self.last_transition
            .insert(to.to_owned(), (at_ns, phase));
    }

    /// Claim both configured heads atomically at a virtual-time boundary.
    /// A failed claim changes neither ownership nor transition chronology.
    pub fn reserve(
        &mut self,
        link_id: &str,
        from: &str,
        to: &str,
        at_ns: u64,
    ) -> Result<(), String> {
        if link_id.trim().is_empty()
            || from == to
            || self.link_heads.contains_key(link_id)
            || [from, to]
                .into_iter()
                .any(|head| !self.contains_head(head) || self.owners.contains_key(head))
        {
            return Err("PAT head lease audit ownership conflict".into());
        }
        self.check_transition(from, to, at_ns, LeaseAuditPhase::Reserve)?;
        self.owners.insert(from.to_owned(), link_id.to_owned());
        self.owners.insert(to.to_owned(), link_id.to_owned());
        self.link_heads
            .insert(link_id.to_owned(), (from.to_owned(), to.to_owned()));
        self.record_transition(from, to, at_ns, LeaseAuditPhase::Reserve);
        Ok(())
    }

    /// Release exactly the active ordered pair at the supplied boundary.
    /// An invalid identity or retroactive release cannot partially mutate
    /// either head or its virtual-time history.
    pub fn release(
        &mut self,
        link_id: &str,
        from: &str,
        to: &str,
        at_ns: u64,
    ) -> Result<(), String> {
        if self
            .link_heads
            .get(link_id)
            .is_none_or(|(a, b)| a != from || b != to)
            || self.owners.get(from).map(String::as_str) != Some(link_id)
            || self.owners.get(to).map(String::as_str) != Some(link_id)
        {
            return Err("PAT head lease audit release mismatch".into());
        }
        self.check_transition(from, to, at_ns, LeaseAuditPhase::Release)?;
        self.owners.remove(from);
        self.owners.remove(to);
        self.link_heads.remove(link_id);
        self.record_transition(from, to, at_ns, LeaseAuditPhase::Release);
        Ok(())
    }

    /// Check the canonical sample's lease projection and virtual-time order.
    /// Failed commands advance event chronology, not successful command
    /// chronology; successful paired commands advance both heads atomically.
    pub fn sample(
        &mut self,
        link_id: &str,
        from: &str,
        to: &str,
        at_ns: u64,
        leased: bool,
        succeeded: bool,
    ) -> Result<(), String> {
        if from == to || !self.contains_head(from) || !self.contains_head(to) {
            return Err("unknown BODY-frame runtime head".into());
        }
        if leased
            && self
                .link_heads
                .get(link_id)
                .is_none_or(|(a, b)| a != from || b != to)
        {
            return Err("PAT head lease audit command ownership mismatch".into());
        }
        if !leased && self.link_heads.contains_key(link_id) {
            return Err("PAT head lease audit command ownership mismatch".into());
        }
        let expected_owner = leased.then_some(link_id);
        for head in [from, to] {
            if self.owners.get(head).map(String::as_str) != expected_owner {
                return Err("PAT head lease audit command ownership mismatch".into());
            }
            if succeeded
                && self
                    .last_success_ns
                    .get(head)
                    .is_some_and(|previous| *previous >= at_ns)
            {
                return Err("PAT head lease audit sample order mismatch".into());
            }
        }
        self.check_transition(from, to, at_ns, LeaseAuditPhase::Sample)?;
        if succeeded {
            self.last_success_ns.insert(from.to_owned(), at_ns);
            self.last_success_ns.insert(to.to_owned(), at_ns);
        }
        self.record_transition(from, to, at_ns, LeaseAuditPhase::Sample);
        Ok(())
    }
}

impl BodyHeadRuntime {
    /// Build an independent lease-only audit from this runtime's real head
    /// registry; predicted boresight state remains owned by this runtime.
    pub fn lease_audit(&self) -> BodyHeadLeaseAudit {
        BodyHeadLeaseAudit {
            known_heads: self.heads.keys().cloned().collect(),
            ..BodyHeadLeaseAudit::default()
        }
    }

    pub fn new(actuators: &[OpticalHeadSlewActuator]) -> Result<Self, String> {
        let mut heads = HashMap::new();
        for actuator in actuators {
            if actuator.head_id.trim().is_empty() || heads.contains_key(&actuator.head_id) {
                return Err("invalid or duplicate BODY-frame runtime head".into());
            }
            let follower = BodyHeadFollower::new(
                actuator.initial_direction_body,
                actuator.max_slew_rate_deg_per_s,
                actuator.max_slew_acceleration_deg_per_s2,
            )?;
            heads.insert(actuator.head_id.clone(), follower);
        }
        Ok(Self {
            heads,
            history: HashMap::new(),
            owners: HashMap::new(),
            link_heads: HashMap::new(),
            lease_start_ns: HashMap::new(),
            release_floor_ns: HashMap::new(),
        })
    }

    fn staged_observation(
        &self,
        head_id: &str,
        at_ns: u64,
        target_body: [f64; 3],
    ) -> Result<(BodyHeadFollower, [f64; 3]), String> {
        if self
            .release_floor_ns
            .get(head_id)
            .is_some_and(|released_at| at_ns < *released_at)
        {
            return Err("BODY-frame runtime command precedes lease release".into());
        }
        if self
            .history
            .get(head_id)
            .and_then(|history| history.last())
            .is_some_and(|(previous, _)| at_ns <= *previous)
        {
            return Err("BODY-frame runtime head samples must increase".into());
        }
        let mut follower = self
            .heads
            .get(head_id)
            .ok_or("unknown BODY-frame runtime head")?
            .clone();
        let boresight = follower.observe(at_ns, target_body)?;
        Ok((follower, boresight))
    }

    fn commit_observation(&mut self, head_id: &str, at_ns: u64, follower: BodyHeadFollower) {
        self.heads.insert(head_id.to_owned(), follower.clone());
        self.history
            .entry(head_id.to_owned())
            .or_default()
            .push((at_ns, follower));
    }

    /// Apply an unleased head's sampled command. Leased heads must use the
    /// link-scoped paired API so a caller cannot bypass command ownership.
    pub fn observe(
        &mut self,
        head_id: &str,
        at_ns: u64,
        target_body: [f64; 3],
    ) -> Result<[f64; 3], String> {
        if self.owners.contains_key(head_id) {
            return Err("BODY-frame runtime head has a link command owner".into());
        }
        let (follower, boresight) = self.staged_observation(head_id, at_ns, target_body)?;
        self.commit_observation(head_id, at_ns, follower);
        Ok(boresight)
    }

    /// Preserve the original unleased paired API for standalone followers.
    /// A leased head cannot be modified without its link identity.
    pub fn observe_pair(
        &mut self,
        from_head_id: &str,
        to_head_id: &str,
        at_ns: u64,
        from_target_body: [f64; 3],
        to_target_body: [f64; 3],
    ) -> Result<([f64; 3], [f64; 3]), String> {
        if self.owners.contains_key(from_head_id) || self.owners.contains_key(to_head_id) {
            return Err("BODY-frame runtime head has a link command owner".into());
        }
        self.observe_pair_unchecked(
            from_head_id,
            to_head_id,
            at_ns,
            from_target_body,
            to_target_body,
        )
    }

    /// Atomically claim both heads for one link and commit a sampled command.
    /// A conflicting link or invalid endpoint leaves both leases and follower
    /// histories unchanged. A link may continue to command its own heads.
    pub fn observe_pair_for_link(
        &mut self,
        link_id: &str,
        from_head_id: &str,
        to_head_id: &str,
        at_ns: u64,
        from_target_body: [f64; 3],
        to_target_body: [f64; 3],
    ) -> Result<([f64; 3], [f64; 3]), String> {
        if link_id.trim().is_empty() {
            return Err("BODY-frame runtime link ID must be nonempty".into());
        }
        if self
            .link_heads
            .get(link_id)
            .is_some_and(|(from, to)| from != from_head_id || to != to_head_id)
        {
            return Err("BODY-frame runtime link cannot change leased endpoints".into());
        }
        for head_id in [from_head_id, to_head_id] {
            if self
                .owners
                .get(head_id)
                .is_some_and(|owner| owner != link_id)
            {
                return Err("BODY-frame runtime head owned by another link".into());
            }
        }
        if self
            .lease_start_ns
            .get(link_id)
            .is_some_and(|start_ns| at_ns < *start_ns)
        {
            return Err("BODY-frame runtime command precedes lease acquisition".into());
        }
        let observed = self.observe_pair_unchecked(
            from_head_id,
            to_head_id,
            at_ns,
            from_target_body,
            to_target_body,
        )?;
        self.lease_start_ns
            .entry(link_id.to_owned())
            .or_insert(at_ns);
        self.owners
            .insert(from_head_id.to_owned(), link_id.to_owned());
        self.owners
            .insert(to_head_id.to_owned(), link_id.to_owned());
        self.link_heads.insert(
            link_id.to_owned(),
            (from_head_id.to_owned(), to_head_id.to_owned()),
        );
        Ok(observed)
    }

    /// Reserve both heads at a PAT reservation start without inventing a
    /// sampled actuator command. This transition is atomic and enforces the
    /// previous owner's release barrier on both heads.
    pub fn reserve_pair_for_link(
        &mut self,
        link_id: &str,
        from_head_id: &str,
        to_head_id: &str,
        at_ns: u64,
    ) -> Result<(), String> {
        if link_id.trim().is_empty()
            || from_head_id == to_head_id
            || self.link_heads.contains_key(link_id)
        {
            return Err("invalid or already active BODY-frame pair reservation".into());
        }
        // A reservation at the same instant as an already committed
        // sample would retroactively claim that head. The canonical
        // PAT order is Release -> Reserve -> Sample.
        for head_id in [from_head_id, to_head_id] {
            if !self.heads.contains_key(head_id)
                || self.owners.contains_key(head_id)
                || self
                    .release_floor_ns
                    .get(head_id)
                    .is_some_and(|floor| at_ns < *floor)
                || self
                    .history
                    .get(head_id)
                    .and_then(|samples| samples.last())
                    .is_some_and(|(sample_ns, _)| at_ns <= *sample_ns)
            {
                return Err(
                    "BODY-frame pair reservation conflicts with head history or owner".into(),
                );
            }
        }
        for head_id in [from_head_id, to_head_id] {
            self.owners.insert(head_id.to_owned(), link_id.to_owned());
        }
        self.link_heads.insert(
            link_id.to_owned(),
            (from_head_id.to_owned(), to_head_id.to_owned()),
        );
        self.lease_start_ns.insert(link_id.to_owned(), at_ns);
        Ok(())
    }

    /// Legacy release at the last committed sample time. Call release_pair_at
    /// when a PAT reservation has a later, explicit virtual-time end boundary.
    pub fn release_pair(
        &mut self,
        link_id: &str,
        from_head_id: &str,
        to_head_id: &str,
    ) -> Result<(), String> {
        let at_ns = [from_head_id, to_head_id]
            .into_iter()
            .filter_map(|head| {
                self.history
                    .get(head)
                    .and_then(|samples| samples.last())
                    .map(|(at_ns, _)| *at_ns)
            })
            .max()
            .unwrap_or(0)
            .max(self.lease_start_ns.get(link_id).copied().unwrap_or(0));
        self.release_pair_at(link_id, from_head_id, to_head_id, at_ns)
    }

    /// Atomically release a link's two command leases at a virtual-time
    /// boundary. Later owners cannot submit commands before this boundary,
    /// even if the previous owner's final observation was much earlier.
    /// A failed release preserves both owners and their time barriers.
    pub fn release_pair_at(
        &mut self,
        link_id: &str,
        from_head_id: &str,
        to_head_id: &str,
        at_ns: u64,
    ) -> Result<(), String> {
        if link_id.trim().is_empty()
            || from_head_id == to_head_id
            || self
                .link_heads
                .get(link_id)
                .is_none_or(|(from, to)| from != from_head_id || to != to_head_id)
            || self.owners.get(from_head_id).map(String::as_str) != Some(link_id)
            || self.owners.get(to_head_id).map(String::as_str) != Some(link_id)
        {
            return Err("BODY-frame runtime pair lease ownership mismatch".into());
        }
        if self
            .lease_start_ns
            .get(link_id)
            .is_none_or(|start_ns| at_ns < *start_ns)
        {
            return Err("BODY-frame runtime release precedes lease acquisition".into());
        }
        for head_id in [from_head_id, to_head_id] {
            if self
                .history
                .get(head_id)
                .and_then(|samples| samples.last())
                .is_some_and(|(sample_ns, _)| at_ns < *sample_ns)
                || self
                    .release_floor_ns
                    .get(head_id)
                    .is_some_and(|released_at| at_ns < *released_at)
            {
                return Err("BODY-frame runtime lease release precedes head history".into());
            }
        }
        for head_id in [from_head_id, to_head_id] {
            self.owners.remove(head_id);
            self.release_floor_ns.insert(head_id.to_owned(), at_ns);
        }
        self.link_heads.remove(link_id);
        self.lease_start_ns.remove(link_id);
        Ok(())
    }

    fn observe_pair_unchecked(
        &mut self,
        from_head_id: &str,
        to_head_id: &str,
        at_ns: u64,
        from_target_body: [f64; 3],
        to_target_body: [f64; 3],
    ) -> Result<([f64; 3], [f64; 3]), String> {
        if from_head_id == to_head_id {
            return Err("BODY-frame follower endpoints must use distinct heads".into());
        }
        let (from_follower, from_boresight) =
            self.staged_observation(from_head_id, at_ns, from_target_body)?;
        let (to_follower, to_boresight) =
            self.staged_observation(to_head_id, at_ns, to_target_body)?;
        self.commit_observation(from_head_id, at_ns, from_follower);
        self.commit_observation(to_head_id, at_ns, to_follower);
        Ok((from_boresight, to_boresight))
    }

    /// Project from the latest successful command at or before a PAT boundary.
    /// Historical commands and the live head state remain unchanged.
    pub fn predicted_at(&self, head_id: &str, at_ns: u64) -> Result<([f64; 3], [f64; 3]), String> {
        let history = self
            .history
            .get(head_id)
            .ok_or("missing predicted optical head history")?;
        let count = history.partition_point(|(sample_ns, _)| *sample_ns <= at_ns);
        if count == 0 {
            return Err("missing predicted head state at PAT boundary".into());
        }
        history[count - 1].1.predicted_at(at_ns)
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
    if previous.link_id != current.link_id
        || current.at_ns <= previous.at_ns
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
    fn lease_audit_enforces_atomic_handoff_and_successful_sample_chronology() {
        let actuators = ["a", "b", "c"].map(|head_id| OpticalHeadSlewActuator {
            head_id: head_id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let mut audit = runtime.lease_audit();
        assert!(audit.contains_head("a"));
        assert!(!audit.contains_head("missing"));
        audit.reserve("first", "a", "b", 10).unwrap();
        assert!(audit.reserve("second", "a", "c", 10).is_err());
        assert!(audit.release("first", "a", "c", 11).is_err());
        assert!(audit.sample("second", "a", "c", 10, false, true).is_err());
        audit.sample("first", "a", "b", 10, true, true).unwrap();
        assert!(audit.sample("first", "a", "b", 10, true, true).is_err());
        assert!(audit.release("first", "a", "b", 10).is_err());
        audit.release("first", "a", "b", 11).unwrap();
        audit.reserve("second", "a", "c", 11).unwrap();
        assert!(audit.sample("second", "a", "c", 10, true, false).is_err());
        audit.sample("second", "a", "c", 11, true, false).unwrap();
        audit.sample("second", "a", "c", 11, true, true).unwrap();
        assert!(audit.release("second", "a", "c", 11).is_err());
        audit.release("second", "a", "c", 12).unwrap();
        audit.sample("second", "a", "c", 12, false, true).unwrap();
        assert!(audit.sample("second", "a", "c", 12, false, true).is_err());
    }

    #[test]
    fn lease_audit_rejects_retroactive_transitions_without_partial_mutation() {
        let actuators = ["a", "b", "c"].map(|head_id| OpticalHeadSlewActuator {
            head_id: head_id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let mut audit = runtime.lease_audit();

        audit.reserve("first", "a", "b", 20).unwrap();
        assert_eq!(
            audit.release("first", "a", "b", 19).unwrap_err(),
            "PAT head lease audit transition order mismatch"
        );
        assert!(audit.sample("first", "a", "b", 19, true, false).is_err());
        audit.sample("first", "a", "b", 20, true, false).unwrap();
        assert!(audit.release("first", "a", "b", 20).is_err());
        audit.release("first", "a", "b", 21).unwrap();

        assert!(audit.reserve("second", "a", "c", 20).is_err());
        audit.reserve("second", "a", "c", 21).unwrap();
        audit.sample("second", "a", "c", 21, true, true).unwrap();
        assert!(audit.reserve("third", "b", "c", 21).is_err());
        audit.release("second", "a", "c", 22).unwrap();
        audit.reserve("third", "b", "c", 22).unwrap();
        audit.sample("third", "b", "c", 22, true, true).unwrap();
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
    fn head_runtime_projects_from_last_successful_command_without_mutating_history() {
        let actuators = [
            OpticalHeadSlewActuator {
                head_id: "sat-a/head-1".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200000.0),
                settling_ns: Some(15_000_000),
            },
            OpticalHeadSlewActuator {
                head_id: "sat-b/head-1".into(),
                initial_direction_body: [-1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 3000.0,
                max_slew_acceleration_deg_per_s2: Some(200000.0),
                settling_ns: None,
            },
        ];
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        runtime.observe("sat-a/head-1", 0, [1.0, 0.0, 0.0]).unwrap();
        runtime
            .observe("sat-b/head-1", 0, [-1.0, 0.0, 0.0])
            .unwrap();
        runtime
            .observe("sat-a/head-1", 30_000_000, [0.0, 1.0, 0.0])
            .unwrap();

        let (projected, target) = runtime.predicted_at("sat-a/head-1", 60_000_000).unwrap();
        assert_eq!(target, [0.0, 1.0, 0.0]);
        assert!((projected[0] - std::f64::consts::FRAC_1_SQRT_2).abs() < 1.0e-12);
        assert_eq!(
            runtime.predicted_at("sat-a/head-1", 60_000_000).unwrap(),
            (projected, target)
        );
        assert_eq!(
            runtime.predicted_at("sat-b/head-1", 60_000_000).unwrap().0,
            [-1.0, 0.0, 0.0]
        );
        assert_eq!(
            runtime.observe("sat-a/head-1", 60_000_000, target).unwrap(),
            projected
        );
        assert!(runtime.predicted_at("sat-a/head-1", 29_000_000).is_ok());
        assert!(runtime.observe("sat-a/head-1", 60_000_000, target).is_err());
        assert!(runtime.observe("unknown", 90_000_000, target).is_err());
    }

    #[test]
    fn head_runtime_rejects_duplicate_heads_and_does_not_record_failed_samples() {
        let actuator = OpticalHeadSlewActuator {
            head_id: "sat-a/head-1".into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        };
        assert!(BodyHeadRuntime::new(&[actuator.clone(), actuator.clone()]).is_err());
        let mut runtime = BodyHeadRuntime::new(&[actuator]).unwrap();
        assert!(runtime.predicted_at("sat-a/head-1", 0).is_err());
        runtime.observe("sat-a/head-1", 0, [1.0, 0.0, 0.0]).unwrap();
        assert!(runtime
            .observe("sat-a/head-1", 10_000_000, [f64::NAN, 0.0, 0.0])
            .is_err());
        assert_eq!(
            runtime.predicted_at("sat-a/head-1", 10_000_000).unwrap(),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
        );
        runtime
            .observe("sat-a/head-1", 10_000_000, [0.0, 1.0, 0.0])
            .unwrap();
    }

    #[test]
    fn head_runtime_pair_rolls_back_both_heads_and_allows_same_timestamp_retry() {
        let actuators = [
            OpticalHeadSlewActuator {
                head_id: "a".into(),
                initial_direction_body: [1.0, 0.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
            OpticalHeadSlewActuator {
                head_id: "b".into(),
                initial_direction_body: [0.0, 1.0, 0.0],
                max_slew_rate_deg_per_s: 100.0,
                max_slew_acceleration_deg_per_s2: None,
                settling_ns: None,
            },
        ];
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        assert_eq!(
            runtime
                .observe_pair("a", "b", 0, [1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
                .unwrap(),
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        );
        assert!(runtime
            .observe_pair("a", "b", 10_000_000, [0.0, 0.0, 1.0], [f64::NAN, 0.0, 0.0])
            .is_err());
        assert_eq!(
            runtime.predicted_at("a", 10_000_000).unwrap(),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
        );
        assert_eq!(
            runtime.predicted_at("b", 10_000_000).unwrap(),
            ([0.0, 1.0, 0.0], [0.0, 1.0, 0.0])
        );
        assert!(runtime
            .observe_pair("a", "a", 10_000_000, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0])
            .is_err());
        assert!(runtime
            .observe_pair("a", "missing", 10_000_000, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0])
            .is_err());
        assert_eq!(
            runtime
                .observe_pair("a", "b", 10_000_000, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0])
                .unwrap(),
            ([1.0, 0.0, 0.0], [0.0, 1.0, 0.0])
        );
        assert_eq!(
            runtime.predicted_at("a", 10_000_000).unwrap().1,
            [0.0, 0.0, 1.0]
        );
        assert_eq!(
            runtime.predicted_at("b", 10_000_000).unwrap().1,
            [0.0, 0.0, 1.0]
        );
        assert!(runtime
            .observe_pair("a", "b", 10_000_000, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0])
            .is_err());
    }

    #[test]
    fn head_runtime_link_command_leases_are_atomic_and_releasable() {
        let actuators = ["a", "b", "c"].map(|id| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let initial = [1.0, 0.0, 0.0];
        runtime
            .observe_pair_for_link("isl-a-b", "a", "b", 0, initial, initial)
            .unwrap();
        assert!(runtime
            .observe_pair_for_link("isl-a-c", "a", "c", 1, initial, initial)
            .is_err());
        assert!(runtime.predicted_at("c", 1).is_err());
        assert!(runtime
            .observe_pair_for_link("isl-a-b", "a", "c", 1, initial, initial)
            .is_err());
        assert!(runtime.observe("a", 1, initial).is_err());
        assert!(runtime.observe_pair("a", "b", 1, initial, initial).is_err());
        assert!(runtime.release_pair("isl-a-c", "a", "b").is_err());
        assert!(runtime.release_pair("isl-a-b", "a", "c").is_err());
        assert!(runtime
            .observe_pair_for_link("isl-a-b", "a", "b", 1, initial, [f64::NAN, 0.0, 0.0])
            .is_err());
        assert_eq!(runtime.predicted_at("a", 1).unwrap().1, initial);
        runtime
            .observe_pair_for_link("isl-a-b", "a", "b", 1, initial, initial)
            .unwrap();
        runtime.release_pair("isl-a-b", "a", "b").unwrap();
        assert!(runtime.release_pair("isl-a-b", "a", "b").is_err());
        runtime
            .observe_pair_for_link("isl-a-c", "a", "c", 2, initial, initial)
            .unwrap();
        assert_eq!(runtime.predicted_at("a", 2).unwrap().1, initial);
        assert_eq!(runtime.predicted_at("b", 2).unwrap().1, initial);
        assert_eq!(runtime.predicted_at("c", 2).unwrap().1, initial);
        assert!(runtime
            .observe_pair_for_link("isl-a-b", "a", "b", 3, initial, initial)
            .is_err());
    }

    #[test]
    fn explicit_pat_pair_reservation_is_atomic_and_time_safe() {
        let actuator = |id: &str| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        };
        let mut runtime =
            BodyHeadRuntime::new(&[actuator("a"), actuator("b"), actuator("c")]).unwrap();
        runtime
            .reserve_pair_for_link("first", "a", "b", 10)
            .unwrap();
        assert!(runtime
            .reserve_pair_for_link("second", "a", "c", 11)
            .is_err());
        assert!(runtime
            .reserve_pair_for_link("first", "a", "b", 11)
            .is_err());
        assert!(runtime
            .reserve_pair_for_link("other", "a", "unknown", 11)
            .is_err());
        assert!(runtime.observe("a", 11, [1.0, 0.0, 0.0]).is_err());
        runtime
            .observe_pair_for_link("first", "a", "b", 10, [1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
            .unwrap();
        runtime.release_pair_at("first", "a", "b", 30).unwrap();
        assert!(runtime
            .reserve_pair_for_link("second", "a", "c", 29)
            .is_err());
        runtime
            .reserve_pair_for_link("second", "a", "c", 30)
            .unwrap();
        assert!(runtime.release_pair_at("second", "a", "c", 29).is_err());
        runtime.release_pair_at("second", "a", "c", 40).unwrap();
        assert!(runtime.observe("a", 39, [1.0, 0.0, 0.0]).is_err());
        assert!(runtime.observe("a", 40, [1.0, 0.0, 0.0]).is_ok());
    }

    #[test]
    fn pat_pair_reservation_rejects_retroactive_same_time_sample_ownership() {
        let actuators = ["a", "b", "c"].map(|id| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let target = [1.0, 0.0, 0.0];

        runtime.observe("a", 10, target).unwrap();
        assert_eq!(
            runtime
                .reserve_pair_for_link("first", "a", "b", 10)
                .unwrap_err(),
            "BODY-frame pair reservation conflicts with head history or owner"
        );
        // Failed admission must not acquire either head or alter the sample.
        assert!(runtime.predicted_at("b", 10).is_err());
        assert_eq!(runtime.predicted_at("a", 10).unwrap().1, target);
        runtime.observe("b", 10, target).unwrap();

        // A later acquisition remains valid, including a release/reacquire
        // handoff at one timestamp when no command has been committed there.
        runtime
            .reserve_pair_for_link("first", "a", "b", 11)
            .unwrap();
        runtime
            .observe_pair_for_link("first", "a", "b", 11, target, target)
            .unwrap();
        runtime.release_pair_at("first", "a", "b", 12).unwrap();
        runtime
            .reserve_pair_for_link("second", "a", "c", 12)
            .unwrap();
        runtime.release_pair_at("second", "a", "c", 12).unwrap();
    }

    #[test]
    fn pat_pair_lease_start_rejects_backdated_commands_and_releases() {
        let actuators = ["a", "b", "c"].map(|id| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let target = [1.0, 0.0, 0.0];

        runtime
            .reserve_pair_for_link("first", "a", "b", 100)
            .unwrap();
        assert!(runtime
            .observe_pair_for_link("first", "a", "b", 99, target, target)
            .is_err());
        assert!(runtime.release_pair_at("first", "a", "b", 99).is_err());
        assert!(runtime
            .reserve_pair_for_link("second", "a", "c", 100)
            .is_err());
        assert!(runtime.predicted_at("a", 100).is_err());

        runtime
            .observe_pair_for_link("first", "a", "b", 100, target, target)
            .unwrap();
        runtime.release_pair_at("first", "a", "b", 101).unwrap();
        runtime
            .reserve_pair_for_link("second", "a", "c", 101)
            .unwrap();
        assert!(runtime
            .observe_pair_for_link("second", "a", "c", 100, target, target)
            .is_err());
        assert!(runtime.release_pair_at("second", "a", "c", 100).is_err());

        // Legacy release of an unsampled reservation must use its start time.
        runtime.release_pair("second", "a", "c").unwrap();
        assert!(runtime
            .reserve_pair_for_link("third", "a", "c", 100)
            .is_err());
        runtime
            .reserve_pair_for_link("third", "a", "c", 101)
            .unwrap();
        runtime.release_pair_at("third", "a", "c", 101).unwrap();
        assert_eq!(runtime.predicted_at("a", 101).unwrap().1, target);
        assert!(runtime.predicted_at("c", 101).is_err());
    }

    #[test]
    fn timed_head_lease_release_blocks_retroactive_handoff() {
        let actuators = ["a", "b", "c"].map(|id| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        let target = [1.0, 0.0, 0.0];
        runtime
            .observe_pair_for_link("first", "a", "b", 0, target, target)
            .unwrap();
        runtime
            .observe_pair_for_link("first", "a", "b", 10, target, target)
            .unwrap();
        assert!(runtime.release_pair_at("first", "a", "b", 9).is_err());
        assert!(runtime.release_pair_at("wrong", "a", "b", 50).is_err());
        assert!(runtime
            .observe_pair_for_link("second", "a", "c", 50, target, target)
            .is_err());
        runtime.release_pair_at("first", "a", "b", 50).unwrap();
        assert!(runtime
            .observe_pair_for_link("second", "a", "c", 49, target, target)
            .is_err());
        assert!(runtime.predicted_at("c", 50).is_err());
        assert!(runtime.observe("b", 49, target).is_err());
        runtime
            .observe_pair_for_link("second", "a", "c", 50, target, target)
            .unwrap();
        assert!(runtime.release_pair_at("second", "a", "c", 49).is_err());
        runtime.release_pair_at("second", "a", "c", 100).unwrap();
        assert!(runtime
            .observe_pair_for_link("first", "a", "b", 99, target, target)
            .is_err());
        runtime
            .observe_pair_for_link("first", "a", "b", 100, target, target)
            .unwrap();
        assert_eq!(runtime.predicted_at("a", 100).unwrap().1, target);
        assert_eq!(runtime.predicted_at("b", 100).unwrap().1, target);
        assert_eq!(runtime.predicted_at("c", 100).unwrap().1, target);
    }

    #[test]
    fn head_runtime_pair_rolls_back_on_second_head_ambiguous_motion() {
        let actuators = ["a", "b"].map(|id| OpticalHeadSlewActuator {
            head_id: id.into(),
            initial_direction_body: [1.0, 0.0, 0.0],
            max_slew_rate_deg_per_s: 100.0,
            max_slew_acceleration_deg_per_s2: None,
            settling_ns: None,
        });
        let mut runtime = BodyHeadRuntime::new(&actuators).unwrap();
        runtime
            .observe_pair("a", "b", 0, [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0])
            .unwrap();
        assert!(runtime
            .observe_pair("a", "b", 10_000_000, [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0])
            .is_err());
        assert_eq!(
            runtime.predicted_at("a", 10_000_000).unwrap(),
            ([1.0, 0.0, 0.0], [1.0, 0.0, 0.0])
        );
        assert_eq!(
            runtime
                .observe_pair("a", "b", 3_000_000_000, [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0],)
                .unwrap()
                .0,
            [1.0, 0.0, 0.0]
        );
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
        // Cross-link observations must never be interpreted as a LOS
        // trajectory, even if their geometry and timestamps are otherwise valid.
        let mut other_link = initial.clone();
        other_link.link_id = "isl-other".into();
        other_link.at_ns = 30_000_000;
        assert!(!geometry_los_rate_allowed(&initial, &other_link, 0.0));
        let mut later_original = initial.clone();
        later_original.at_ns = 60_000_000;
        assert!(!geometry_los_rate_allowed(
            &other_link,
            &later_original,
            0.0
        ));
        assert!(geometry_los_rate_allowed(&initial, &later_original, 0.0));

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
