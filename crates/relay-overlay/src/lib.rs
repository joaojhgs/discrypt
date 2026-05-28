//! Adaptive relay overlay foundations.
//!
//! The overlay is transport-agnostic and content-blind: it ranks relay peers,
//! selects bounded-hop routes, reroutes around failures, tracks redelivery and
//! replay state, and stores only ciphertext envelopes for opportunistic
//! store-and-forward.

pub mod failover;
pub mod integrity;
pub mod ranking;
pub mod redelivery;
pub mod store_forward;
pub mod topology;

pub use ranking::{rank, score, RelayMetrics};
pub use topology::hop_limit_ok;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatibility_exports_rank_and_hop_limit() {
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
