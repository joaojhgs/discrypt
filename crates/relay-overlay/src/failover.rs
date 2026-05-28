//! Relay failover selection.

use crate::topology::{RelayRoute, RelayTopology, TopologyError};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Result of a local reroute after relay failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailoverDecision {
    /// Original route before the failure.
    pub previous: RelayRoute,
    /// Replacement route that avoids the failed peer.
    pub replacement: RelayRoute,
    /// Failed peer that triggered failover.
    pub failed_peer: String,
    /// Simulated convergence time for the deterministic harness.
    pub convergence_ms: u64,
}

impl FailoverDecision {
    /// Phase-2 gate: convergence must be within 3 seconds.
    #[must_use]
    pub fn converged_within_phase2_gate(&self) -> bool {
        self.convergence_ms <= 3_000
    }
}

/// Reroute away from a failed relay while preserving hop limits.
pub fn reroute_after_failure(
    topology: &RelayTopology,
    previous: RelayRoute,
    failed_peer: &str,
    convergence_ms: u64,
) -> Result<FailoverDecision, TopologyError> {
    let source = previous
        .path
        .first()
        .cloned()
        .ok_or(TopologyError::NoRoute)?;
    let destination = previous
        .path
        .last()
        .cloned()
        .ok_or(TopologyError::NoRoute)?;
    let avoided = BTreeSet::from([failed_peer.to_owned()]);
    let replacement = topology.route_avoiding(&source, &destination, &avoided)?;
    Ok(FailoverDecision {
        previous,
        replacement,
        failed_peer: failed_peer.to_owned(),
        convergence_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ranking::RelayMetrics;

    fn metrics(peer_id: &str, latency_ms: u32) -> RelayMetrics {
        RelayMetrics {
            peer_id: peer_id.to_owned(),
            latency_ms,
            stability: 1.0,
            battery_cost: 0.0,
            freeload_penalty: 0.0,
        }
    }

    #[test]
    fn failover_avoids_failed_relay_within_gate() -> Result<(), TopologyError> {
        let mut topology = RelayTopology::default();
        for peer in ["alice", "primary", "backup", "bob"] {
            topology.upsert_peer(metrics(peer, if peer == "backup" { 40 } else { 10 }));
        }
        topology.connect("alice", "primary")?;
        topology.connect("primary", "bob")?;
        topology.connect("alice", "backup")?;
        topology.connect("backup", "bob")?;

        let previous = topology.route("alice", "bob")?;
        let decision = reroute_after_failure(&topology, previous, "primary", 2_750)?;
        assert_eq!(decision.replacement.path, ["alice", "backup", "bob"]);
        assert!(decision.converged_within_phase2_gate());
        assert!(!decision.replacement.contains_peer("primary"));
        Ok(())
    }
}
