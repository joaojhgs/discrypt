//! Adaptive relay overlay foundations.
//!
//! The overlay is transport-agnostic and content-blind: it ranks relay peers,
//! selects bounded-hop routes, reroutes around failures, tracks redelivery and
//! replay state, and stores only ciphertext envelopes for opportunistic
//! store-and-forward.

//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod capability;
pub mod failover;
pub mod gossip;
pub mod integrity;
pub mod manager;
pub mod production_status;
pub mod ranking;
pub mod redelivery;
pub mod store_forward;
pub mod topology;

pub use capability::{
    BatteryDozePosture, CapabilityAdvertisementBook, CapabilityAdvertisementError,
    RelayCapabilityAdvertisement, RelayCapacityAdvertisement,
};
pub use gossip::{GossipItem, GossipMesh, GossipNode};
pub use integrity::{RelayPacket, RelayPayloadKind, RelayProtectedEnvelope};
pub use manager::{
    ChurnDampingPolicy, ConstructedOverlayRoute, FailoverRecoveryPolicy, MediaConcealmentReport,
    OverlayFailoverReport, OverlayManager, OverlayManagerError, OverlayRouteDecision,
    OverlayRouteUse, RelayRuntimeObservation, TopologyChangeReason, TopologyChangeReport,
};
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
