use serde::Deserialize;
use std::collections::HashSet;

#[derive(Debug, Deserialize)]
pub struct Node {
    pub id: String,
    #[serde(default)]
    pub latitude: f64,
    #[serde(default)]
    pub longitude: f64,
}

#[derive(Debug, Deserialize)]
pub struct Scenario {
    pub name: String,
    pub satellites: Vec<Node>,
    pub terminals: Vec<Node>,
    pub gateways: Vec<Node>,
}

impl Scenario {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("scenario name required".into());
        }
        if self.satellites.is_empty() || self.terminals.is_empty() || self.gateways.is_empty() {
            return Err("scenario needs satellites, terminals and gateways".into());
        }
        let mut ids = HashSet::new();
        for node in self
            .satellites
            .iter()
            .chain(&self.terminals)
            .chain(&self.gateways)
        {
            if node.id.trim().is_empty() {
                return Err("node id required".into());
            }
            if !ids.insert(&node.id) {
                return Err(format!("duplicate node id: {}", node.id));
            }
            if !(-90.0..=90.0).contains(&node.latitude)
                || !(-180.0..=180.0).contains(&node.longitude)
            {
                return Err(format!("invalid coordinates for {}", node.id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_nodes_are_rejected() {
        let raw = r#"{"name":"test","satellites":[{"id":"sat"}],"terminals":[{"id":"ue"}],"gateways":[{"id":"sat"}]}"#;
        let s: Scenario = serde_json::from_str(raw).unwrap();
        assert!(s.validate().is_err());
    }
}
