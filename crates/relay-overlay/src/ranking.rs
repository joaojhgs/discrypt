//! Deterministic relay ranking.
//!
//! Ranking is deliberately local and content-blind. It combines latency,
//! stability, energy cost, and freeload accounting into a deterministic order so
//! topology/failover tests can assert exact relay choices.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Relay candidate metrics.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelayMetrics {
    /// Stable peer identifier.
    pub peer_id: String,
    /// Recent round-trip latency estimate.
    pub latency_ms: u32,
    /// Local stability score in the inclusive 0.0..=1.0 range.
    pub stability: f32,
    /// Local estimate of battery/CPU/network cost.
    pub battery_cost: f32,
    /// Anti-freeload penalty; higher values deprioritize peers that consume but
    /// do not contribute relay capacity.
    pub freeload_penalty: f32,
}

/// Score a relay candidate. Higher is better.
#[must_use]
pub fn score(metrics: &RelayMetrics) -> f32 {
    (1000.0 / (metrics.latency_ms.max(1) as f32)) + metrics.stability
        - metrics.battery_cost
        - metrics.freeload_penalty
}

/// Deterministically rank relay candidates by score, then peer id.
#[must_use]
pub fn rank(mut peers: Vec<RelayMetrics>) -> Vec<RelayMetrics> {
    peers.sort_by(compare_metrics);
    peers
}

/// Comparator used consistently by topology and failover selection.
#[must_use]
pub fn compare_metrics(a: &RelayMetrics, b: &RelayMetrics) -> Ordering {
    score(b)
        .partial_cmp(&score(a))
        .unwrap_or(Ordering::Equal)
        .then_with(|| a.peer_id.cmp(&b.peer_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metrics(peer_id: &str, latency_ms: u32, stability: f32, penalty: f32) -> RelayMetrics {
        RelayMetrics {
            peer_id: peer_id.to_owned(),
            latency_ms,
            stability,
            battery_cost: 0.0,
            freeload_penalty: penalty,
        }
    }

    #[test]
    fn ranks_low_latency_stable_peer_and_penalizes_freeloading() {
        let peers = rank(vec![
            metrics("freeloader", 10, 1.0, 500.0),
            metrics("stable", 20, 1.0, 0.0),
            metrics("slow", 200, 1.0, 0.0),
        ]);
        assert_eq!(peers[0].peer_id, "stable");
        assert_eq!(peers[2].peer_id, "freeloader");
    }

    #[test]
    fn ties_are_stable_by_peer_id() {
        let peers = rank(vec![metrics("b", 20, 1.0, 0.0), metrics("a", 20, 1.0, 0.0)]);
        assert_eq!(peers[0].peer_id, "a");
    }
}
