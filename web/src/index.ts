export interface ConstellationNode {
  id: string;
  latitude?: number;
  longitude?: number;
}

export interface Scenario {
  name: string;
  satellites: ConstellationNode[];
  terminals: ConstellationNode[];
  gateways: ConstellationNode[];
}

export function summarizeScenario(s: Scenario): string {
  return `${s.name}: ${s.satellites.length} satellites, ${s.terminals.length} terminals, ${s.gateways.length} gateways`;
}
