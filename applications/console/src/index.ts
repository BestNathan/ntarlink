export type NodeKind = "terminal" | "satellite" | "ground_station" | "pop" | "internet";
export type LinkKind = "user_rf" | "optical_isl" | "feeder" | "backhaul" | "internet";
export interface Node { id: string; kind: NodeKind; latitude_deg?: number; longitude_deg?: number }
export interface Link { id: string; from: string; to: string; kind: LinkKind; capacity_bps: number; propagation_delay_ns: number; queue_limit_bytes: number }
export interface Scenario { schema_version: 1; name: string; duration_ns: number; nodes: Node[]; links: Link[] }
export interface HopTrace { link_id: string; from: string; to: string; enqueued_ns: number; arrived_ns: number; queue_delay_ns: number; serialization_ns: number; propagation_ns: number }
export interface PacketTrace { packet_id: string; flow_id: string; status: "delivered" | "dropped" | "in_flight"; drop_reason: string | null; hops: HopTrace[] }
export interface SimulationReport { scenario: string; fidelity: string; injected: number; delivered: number; dropped: number; packets: PacketTrace[] }
export function summarizeScenario(s: Scenario): string { return `${s.name}: ${s.nodes.length} nodes, ${s.links.length} links`; }
export function packetJourney(p: PacketTrace): string { return p.hops.length ? [p.hops[0].from, ...p.hops.map(h => h.to)].join(" -> ") : "(no hops)"; }
export function deliveryRatio(r: SimulationReport): number { return r.injected ? r.delivered / r.injected : 0; }
