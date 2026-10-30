//! SYS-02 deterministic optical-head availability, not physical pointing.
use ntarlink_contracts::OpticalHeadOutage;
use std::collections::HashMap;

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
        assert!(availability.available_intervals("a", "b", 50, 50).is_empty());
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
