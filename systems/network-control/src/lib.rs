use ntarlink_contracts::Link;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

fn link_cost_ns(link: &Link, packet_size_bytes: u64) -> Option<u64> {
    let bits_ns = u128::from(packet_size_bytes) * 8 * 1_000_000_000;
    let bitrate = u128::from(link.capacity_bps);
    let serialization = bits_ns.div_ceil(bitrate);
    let total = serialization
        + u128::from(link.propagation_delay_ns)
        + u128::from(link.processing_delay_ns);
    u64::try_from(total).ok()
}

/// Compute an installed next-hop table toward one destination.
/// A next hop is a directed link index from the scenario's link array.
pub fn next_hops(
    links: &[Link],
    available: &[bool],
    destination: &str,
    packet_size_bytes: u64,
) -> HashMap<String, usize> {
    let mut incoming: HashMap<&str, Vec<usize>> = HashMap::new();
    for (index, link) in links.iter().enumerate() {
        if available.get(index) == Some(&true) {
            incoming.entry(&link.to).or_default().push(index);
        }
    }
    for choices in incoming.values_mut() {
        choices.sort_by(|&a, &b| links[a].id.cmp(&links[b].id));
    }
    let mut distances: HashMap<String, u64> = HashMap::new();
    let mut routes: HashMap<String, usize> = HashMap::new();
    let mut heap = BinaryHeap::new();
    distances.insert(destination.to_owned(), 0);
    heap.push(Reverse((0u64, destination.to_owned())));
    while let Some(Reverse((cost, current))) = heap.pop() {
        if distances.get(&current) != Some(&cost) {
            continue;
        }
        if let Some(candidate_links) = incoming.get(current.as_str()) {
            for &index in candidate_links {
                let link = &links[index];
                let Some(edge_cost) = link_cost_ns(link, packet_size_bytes) else {
                    continue;
                };
                let Some(next_cost) = cost.checked_add(edge_cost) else {
                    continue;
                };
                let replace = match distances.get(&link.from) {
                    None => true,
                    Some(previous) if next_cost < *previous => true,
                    Some(previous) if next_cost == *previous => routes
                        .get(&link.from)
                        .is_none_or(|old| link.id < links[*old].id),
                    _ => false,
                };
                if replace {
                    distances.insert(link.from.clone(), next_cost);
                    routes.insert(link.from.clone(), index);
                    heap.push(Reverse((next_cost, link.from.clone())));
                }
            }
        }
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;
    use ntarlink_contracts::Scenario;

    #[test]
    fn direct_route_preferred_until_link_down() {
        let s = Scenario::from_json(include_str!("../../../scenarios/isl-failure.json")).unwrap();
        let up: Vec<_> = s.links.iter().map(|x| x.up).collect();
        let route = next_hops(&s.links, &up, "internet", 1200);
        let direct_index = route["sat-a"];
        assert_eq!(s.links[direct_index].id, "isl-a-b");
        let mut down = up;
        down[direct_index] = false;
        let reroute = next_hops(&s.links, &down, "internet", 1200);
        assert_eq!(s.links[reroute["sat-a"]].id, "isl-a-c");
    }

    #[test]
    fn no_path_when_feeder_down() {
        let s = Scenario::from_json(include_str!("../../../scenarios/minimal.json")).unwrap();
        let mut up: Vec<_> = s.links.iter().map(|x| x.up).collect();
        let feeder = s.links.iter().position(|x| x.id == "feeder-b-gw").unwrap();
        up[feeder] = false;
        assert!(!next_hops(&s.links, &up, "internet", 1200).contains_key("ue-a"));
    }
}
