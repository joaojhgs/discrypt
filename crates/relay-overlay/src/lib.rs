//! Adaptive relay overlay foundations.
pub mod integrity;

use serde::{Deserialize, Serialize};

/// Relay candidate metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayMetrics {
    pub peer_id: String,
    pub latency_ms: u32,
    pub stability: f32,
    pub battery_cost: f32,
    pub freeload_penalty: f32,
}
#[must_use]
pub fn score(m: &RelayMetrics) -> f32 {
    (1000.0 / (m.latency_ms.max(1) as f32)) + m.stability - m.battery_cost - m.freeload_penalty
}
#[must_use]
pub fn rank(mut peers: Vec<RelayMetrics>) -> Vec<RelayMetrics> {
    peers.sort_by(|a, b| {
        score(b)
            .partial_cmp(&score(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    peers
}
#[must_use]
pub fn hop_limit_ok(hops: usize) -> bool {
    hops <= 3
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranks_low_latency_stable_peer() {
        let peers = rank(vec![
            RelayMetrics {
                peer_id: "bad".into(),
                latency_ms: 100,
                stability: 0.1,
                battery_cost: 0.0,
                freeload_penalty: 0.0,
            },
            RelayMetrics {
                peer_id: "good".into(),
                latency_ms: 10,
                stability: 1.0,
                battery_cost: 0.0,
                freeload_penalty: 0.0,
            },
        ]);
        assert_eq!(peers[0].peer_id, "good");
        assert!(!hop_limit_ok(4));
    }
}
