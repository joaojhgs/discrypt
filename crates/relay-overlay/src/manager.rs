//! Live overlay manager fed by runtime relay observations.
//!
//! The deterministic topology, ranking, and failover modules remain pure, but
//! production callers need a stateful owner that ingests measured peer health and
//! route observations instead of relying on static test fixtures. This module is
//! that boundary: it accepts runtime metrics from transport/media/text services,
//! updates the content-blind relay topology, and returns auditable route/failover
//! decisions without inspecting application plaintext.

use crate::capability::{
    CapabilityAdvertisementBook, CapabilityAdvertisementError, RelayCapabilityAdvertisement,
};
use crate::failover::{reroute_after_failure, FailoverDecision};
use crate::ranking::RelayMetrics;
use crate::topology::{RelayRoute, RelayTopology, TopologyError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Runtime measurements collected for one potential relay peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayRuntimeObservation {
    /// Stable peer identifier from the authenticated overlay peer set.
    pub peer_id: String,
    /// Last measured round-trip latency in milliseconds.
    pub latency_ms: u32,
    /// Successful relay health probes in the current scoring window.
    pub successful_probes: u32,
    /// Failed relay health probes in the current scoring window.
    pub failed_probes: u32,
    /// Estimated local battery/CPU/network cost in basis points.
    pub battery_cost_bps: u16,
    /// Relay bytes this peer has contributed in the current accounting window.
    pub contributed_bytes: u64,
    /// Relay bytes this peer has consumed in the current accounting window.
    pub consumed_bytes: u64,
}

impl RelayRuntimeObservation {
    /// Convert runtime observations into deterministic ranking metrics.
    #[must_use]
    pub fn to_ranking_metrics(&self) -> RelayMetrics {
        let total_probes = self
            .successful_probes
            .saturating_add(self.failed_probes)
            .max(1);
        let stability = (self.successful_probes as f32 / total_probes as f32).clamp(0.0, 1.0);
        let battery_cost = self.battery_cost_bps as f32 / 10_000.0;
        let freeload_penalty = if self.contributed_bytes == 0 && self.consumed_bytes > 0 {
            1000.0
        } else if self.consumed_bytes > self.contributed_bytes {
            let deficit = self.consumed_bytes.saturating_sub(self.contributed_bytes) as f32;
            let denominator = self.contributed_bytes.max(1) as f32;
            (deficit / denominator).min(1000.0)
        } else {
            0.0
        };
        RelayMetrics {
            peer_id: self.peer_id.clone(),
            latency_ms: self.latency_ms.max(1),
            stability,
            battery_cost,
            freeload_penalty,
        }
    }
}

/// Route decision produced by the live overlay manager.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverlayRouteDecision {
    /// Selected content-blind relay route.
    pub route: RelayRoute,
    /// Ranked next-hop candidates visible to the source at decision time.
    pub ranked_source_neighbors: Vec<String>,
    /// Sequence number of the manager state used for the decision.
    pub manager_epoch: u64,
}

/// Failover decision plus manager epoch metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OverlayFailoverReport {
    /// Reroute details returned by the deterministic failover module.
    pub decision: FailoverDecision,
    /// Recovery SLA used to accept or reject the failover.
    pub recovery_policy: FailoverRecoveryPolicy,
    /// Media concealment/gap report when failover protected a media route.
    pub media_concealment: Option<MediaConcealmentReport>,
    /// Sequence number of the manager state used for the decision.
    pub manager_epoch: u64,
}

/// Runtime failover SLA for overlay route recovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FailoverRecoveryPolicy {
    /// Maximum accepted route convergence time in milliseconds.
    pub max_failover_ms: u64,
    /// Maximum tolerated media gap after concealment in milliseconds.
    pub max_media_gap_ms: u64,
    /// Nominal concealment frame size used to size concealment work.
    pub concealment_frame_ms: u64,
}

impl Default for FailoverRecoveryPolicy {
    fn default() -> Self {
        Self {
            max_failover_ms: 3_000,
            max_media_gap_ms: 200,
            concealment_frame_ms: 20,
        }
    }
}

/// Media gap/concealment result for a protected failover.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MediaConcealmentReport {
    /// Observed media gap after transport/overlay recovery.
    pub observed_gap_ms: u64,
    /// Maximum tolerated media gap.
    pub target_gap_ms: u64,
    /// Number of concealment frames needed to bridge the observed gap.
    pub concealment_frames: u64,
    /// Whether the media gap stayed inside the target.
    pub target_met: bool,
}

/// Overlay route payload class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayRouteUse {
    /// Text messages and command/control frames.
    TextControl,
    /// Protected voice/media frames.
    VoiceMedia,
}

impl OverlayRouteUse {
    /// Minimum egress budget required from every relay hop.
    #[must_use]
    pub const fn min_egress_bytes_per_second(self) -> u64 {
        match self {
            Self::TextControl => 1_024,
            Self::VoiceMedia => 32_000,
        }
    }
}

/// Capacity-checked overlay route ready for text/control or voice media.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConstructedOverlayRoute {
    /// Route use case.
    pub use_case: OverlayRouteUse,
    /// Selected content-blind route.
    pub route: RelayRoute,
    /// Number of relay hops/edges in the route.
    pub hop_count: usize,
    /// Minimum relay egress capacity seen across intermediate relay hops.
    pub bottleneck_egress_bytes_per_second: Option<u64>,
    /// Manager epoch used for route construction.
    pub manager_epoch: u64,
}

/// Reason a topology edge is being changed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TopologyChangeReason {
    /// Initial or trusted topology setup; not rate-limited.
    Bootstrap,
    /// Planned reparent caused by metric/ranking churn; rate-limited.
    PlannedReparent,
    /// Hard failure recovery; bypasses churn damping.
    HardFailure,
}

/// Damping policy for planned topology churn.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChurnDampingPolicy {
    /// Minimum milliseconds between planned reparent/topology changes.
    pub min_planned_change_interval_ms: u64,
}

impl Default for ChurnDampingPolicy {
    fn default() -> Self {
        Self {
            min_planned_change_interval_ms: 30_000,
        }
    }
}

/// Accepted topology change report.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TopologyChangeReport {
    /// Change reason.
    pub reason: TopologyChangeReason,
    /// Endpoint A in the new/updated edge.
    pub a: String,
    /// Endpoint B in the new/updated edge.
    pub b: String,
    /// Manager epoch after the accepted change.
    pub manager_epoch: u64,
    /// Next time a planned reparent may be accepted.
    pub next_planned_change_allowed_at_ms: Option<u64>,
}

/// Errors returned by [`OverlayManager`].
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum OverlayManagerError {
    /// Runtime observation was malformed or unauthenticated by the caller.
    #[error("invalid relay runtime observation: {0}")]
    InvalidObservation(String),
    /// Underlying topology operation failed.
    #[error(transparent)]
    Topology(#[from] TopologyError),
    /// Capability advertisement validation/storage failed.
    #[error(transparent)]
    Capability(#[from] CapabilityAdvertisementError),
    /// Route cannot satisfy hop, fanout, or capacity policy.
    #[error("overlay route capacity check failed: {0}")]
    RouteCapacity(String),
    /// Planned topology change was blocked by churn damping.
    #[error("planned overlay topology change damped until {next_allowed_at_ms}ms")]
    ChurnDamped {
        /// Earliest time the planned change may be retried.
        next_allowed_at_ms: u64,
    },
    /// Failover convergence exceeded the release SLA.
    #[error("overlay failover exceeded {max_failover_ms}ms: observed {observed_ms}ms")]
    FailoverSlaExceeded {
        /// Observed route convergence time.
        observed_ms: u64,
        /// Maximum allowed convergence time.
        max_failover_ms: u64,
    },
    /// Media gap after failover exceeded the concealment target.
    #[error("media failover gap exceeded {max_gap_ms}ms: observed {observed_gap_ms}ms")]
    MediaGapExceeded {
        /// Observed media gap after concealment/recovery.
        observed_gap_ms: u64,
        /// Maximum tolerated gap.
        max_gap_ms: u64,
    },
}

/// Stateful overlay routing manager fed by runtime metrics.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OverlayManager {
    topology: RelayTopology,
    observations: BTreeMap<String, RelayRuntimeObservation>,
    capabilities: CapabilityAdvertisementBook,
    churn_policy: ChurnDampingPolicy,
    last_planned_topology_change_ms: Option<u64>,
    failed_peers: BTreeSet<String>,
    epoch: u64,
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new(RelayTopology::default())
    }
}

impl OverlayManager {
    /// Create an overlay manager from an existing topology shell.
    #[must_use]
    pub fn new(topology: RelayTopology) -> Self {
        Self {
            topology,
            observations: BTreeMap::new(),
            capabilities: CapabilityAdvertisementBook::default(),
            churn_policy: ChurnDampingPolicy::default(),
            last_planned_topology_change_ms: None,
            failed_peers: BTreeSet::new(),
            epoch: 0,
        }
    }

    /// Current manager sequence number.
    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Number of peers with accepted runtime observations.
    #[must_use]
    pub fn peer_count(&self) -> usize {
        self.observations.len()
    }

    /// Upsert one runtime observation and update deterministic ranking inputs.
    pub fn upsert_observation(
        &mut self,
        observation: RelayRuntimeObservation,
    ) -> Result<(), OverlayManagerError> {
        validate_observation(&observation)?;
        self.topology.upsert_peer(observation.to_ranking_metrics());
        self.failed_peers.remove(&observation.peer_id);
        self.observations
            .insert(observation.peer_id.clone(), observation);
        self.bump_epoch();
        Ok(())
    }

    /// Accept a fresh capability advertisement and feed it into runtime routing metrics.
    pub fn upsert_capability_advertisement(
        &mut self,
        advertisement: RelayCapabilityAdvertisement,
        now_ms: u64,
    ) -> Result<(), OverlayManagerError> {
        let observation = advertisement.to_runtime_observation();
        self.capabilities.accept(advertisement, now_ms)?;
        self.upsert_observation(observation)
    }

    /// Connect two known peers in the local overlay graph.
    pub fn connect_peers(&mut self, a: &str, b: &str) -> Result<(), OverlayManagerError> {
        self.topology.connect(a, b)?;
        self.bump_epoch();
        Ok(())
    }

    /// Set churn damping policy.
    #[must_use]
    pub fn with_churn_policy(mut self, churn_policy: ChurnDampingPolicy) -> Self {
        self.churn_policy = churn_policy;
        self
    }

    /// Connect peers with churn damping for planned reparenting.
    pub fn connect_peers_with_churn_damping(
        &mut self,
        a: &str,
        b: &str,
        now_ms: u64,
        reason: TopologyChangeReason,
    ) -> Result<TopologyChangeReport, OverlayManagerError> {
        if reason == TopologyChangeReason::PlannedReparent {
            if let Some(last) = self.last_planned_topology_change_ms {
                let next_allowed =
                    last.saturating_add(self.churn_policy.min_planned_change_interval_ms);
                if now_ms < next_allowed {
                    return Err(OverlayManagerError::ChurnDamped {
                        next_allowed_at_ms: next_allowed,
                    });
                }
            }
            self.last_planned_topology_change_ms = Some(now_ms);
        }
        self.topology.connect(a, b)?;
        self.bump_epoch();
        let next_planned_change_allowed_at_ms = self
            .last_planned_topology_change_ms
            .map(|last| last.saturating_add(self.churn_policy.min_planned_change_interval_ms));
        Ok(TopologyChangeReport {
            reason,
            a: a.to_owned(),
            b: b.to_owned(),
            manager_epoch: self.epoch,
            next_planned_change_allowed_at_ms,
        })
    }

    /// Return source-neighbor order from the latest runtime observations.
    #[must_use]
    pub fn ranked_neighbors(&self, peer_id: &str) -> Vec<String> {
        self.topology
            .ranked_neighbors(peer_id)
            .into_iter()
            .map(|metrics| metrics.peer_id)
            .collect()
    }

    /// Select a route using the latest runtime observations and failure set.
    pub fn route(
        &self,
        source: &str,
        destination: &str,
    ) -> Result<OverlayRouteDecision, OverlayManagerError> {
        let route = self
            .topology
            .route_avoiding(source, destination, &self.failed_peers)?;
        Ok(OverlayRouteDecision {
            route,
            ranked_source_neighbors: self.ranked_neighbors(source),
            manager_epoch: self.epoch,
        })
    }

    /// Construct a capacity-checked route for text/control or voice media.
    pub fn construct_route(
        &self,
        use_case: OverlayRouteUse,
        source: &str,
        destination: &str,
    ) -> Result<ConstructedOverlayRoute, OverlayManagerError> {
        let decision = self.route(source, destination)?;
        if !decision.route.within_hop_limit() {
            return Err(OverlayManagerError::RouteCapacity(
                "route exceeds three-hop overlay limit".to_owned(),
            ));
        }
        let mut bottleneck = None;
        for relay_peer in intermediate_relays(&decision.route) {
            let advertisement = self.capabilities.get(relay_peer).ok_or_else(|| {
                OverlayManagerError::RouteCapacity(format!(
                    "missing capability advertisement for relay {relay_peer}"
                ))
            })?;
            let degree = self.topology.peer_degree(relay_peer).unwrap_or_default();
            if degree > usize::from(advertisement.relay_capacity.max_fanout) {
                return Err(OverlayManagerError::RouteCapacity(format!(
                    "relay {relay_peer} exceeds advertised fanout"
                )));
            }
            if advertisement.relay_capacity.egress_bytes_per_second
                < use_case.min_egress_bytes_per_second()
            {
                return Err(OverlayManagerError::RouteCapacity(format!(
                    "relay {relay_peer} lacks egress capacity for {use_case:?}"
                )));
            }
            bottleneck = Some(bottleneck.map_or(
                advertisement.relay_capacity.egress_bytes_per_second,
                |current: u64| current.min(advertisement.relay_capacity.egress_bytes_per_second),
            ));
        }
        Ok(ConstructedOverlayRoute {
            use_case,
            hop_count: decision.route.hop_count(),
            route: decision.route,
            bottleneck_egress_bytes_per_second: bottleneck,
            manager_epoch: self.epoch,
        })
    }

    /// Mark a peer failed and produce a bounded-hop replacement route.
    pub fn mark_failed_and_reroute(
        &mut self,
        previous: RelayRoute,
        failed_peer: &str,
        observed_convergence_ms: u64,
    ) -> Result<OverlayFailoverReport, OverlayManagerError> {
        self.mark_failed_and_reroute_with_policy(
            previous,
            failed_peer,
            observed_convergence_ms,
            None,
            FailoverRecoveryPolicy::default(),
        )
    }

    /// Mark a media relay failed and produce a replacement route that satisfies
    /// both the route failover SLA and media concealment/gap target.
    pub fn mark_failed_media_and_reroute(
        &mut self,
        previous: RelayRoute,
        failed_peer: &str,
        observed_convergence_ms: u64,
        observed_media_gap_ms: u64,
    ) -> Result<OverlayFailoverReport, OverlayManagerError> {
        self.mark_failed_and_reroute_with_policy(
            previous,
            failed_peer,
            observed_convergence_ms,
            Some(observed_media_gap_ms),
            FailoverRecoveryPolicy::default(),
        )
    }

    /// Mark a peer failed and produce a bounded-hop replacement route under a
    /// caller-supplied recovery policy.
    pub fn mark_failed_and_reroute_with_policy(
        &mut self,
        previous: RelayRoute,
        failed_peer: &str,
        observed_convergence_ms: u64,
        observed_media_gap_ms: Option<u64>,
        recovery_policy: FailoverRecoveryPolicy,
    ) -> Result<OverlayFailoverReport, OverlayManagerError> {
        if observed_convergence_ms > recovery_policy.max_failover_ms {
            return Err(OverlayManagerError::FailoverSlaExceeded {
                observed_ms: observed_convergence_ms,
                max_failover_ms: recovery_policy.max_failover_ms,
            });
        }
        let media_concealment = observed_media_gap_ms
            .map(|observed_gap_ms| media_concealment_report(observed_gap_ms, recovery_policy))
            .transpose()?;
        self.failed_peers.insert(failed_peer.to_owned());
        let decision = reroute_after_failure(
            &self.topology,
            previous,
            failed_peer,
            observed_convergence_ms,
        )?;
        self.bump_epoch();
        Ok(OverlayFailoverReport {
            decision,
            recovery_policy,
            media_concealment,
            manager_epoch: self.epoch,
        })
    }

    fn bump_epoch(&mut self) {
        self.epoch = self.epoch.saturating_add(1);
    }
}

fn intermediate_relays(route: &RelayRoute) -> impl Iterator<Item = &str> {
    let len = route.path.len();
    route
        .path
        .iter()
        .enumerate()
        .filter(move |(idx, _peer)| *idx != 0 && *idx + 1 != len)
        .map(|(_idx, peer)| peer.as_str())
}

fn validate_observation(observation: &RelayRuntimeObservation) -> Result<(), OverlayManagerError> {
    if observation.peer_id.trim().is_empty() {
        return Err(OverlayManagerError::InvalidObservation(
            "peer id is required".to_owned(),
        ));
    }
    if observation.successful_probes == 0 && observation.failed_probes == 0 {
        return Err(OverlayManagerError::InvalidObservation(
            "at least one relay health probe is required".to_owned(),
        ));
    }
    Ok(())
}

fn media_concealment_report(
    observed_gap_ms: u64,
    policy: FailoverRecoveryPolicy,
) -> Result<MediaConcealmentReport, OverlayManagerError> {
    if observed_gap_ms > policy.max_media_gap_ms {
        return Err(OverlayManagerError::MediaGapExceeded {
            observed_gap_ms,
            max_gap_ms: policy.max_media_gap_ms,
        });
    }
    let frame_ms = policy.concealment_frame_ms.max(1);
    let concealment_frames = observed_gap_ms.div_ceil(frame_ms);
    Ok(MediaConcealmentReport {
        observed_gap_ms,
        target_gap_ms: policy.max_media_gap_ms,
        concealment_frames,
        target_met: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        peer_id: &str,
        latency_ms: u32,
        successful: u32,
        consumed: u64,
    ) -> RelayRuntimeObservation {
        RelayRuntimeObservation {
            peer_id: peer_id.to_owned(),
            latency_ms,
            successful_probes: successful,
            failed_probes: 0,
            battery_cost_bps: 0,
            contributed_bytes: 10_000,
            consumed_bytes: consumed,
        }
    }

    #[test]
    fn live_manager_routes_from_runtime_observations() -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("slow-relay", 200, 10, 0),
            observation("fast-relay", 20, 10, 0),
            observation("bob", 5, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "slow-relay")?;
        manager.connect_peers("slow-relay", "bob")?;
        manager.connect_peers("alice", "fast-relay")?;
        manager.connect_peers("fast-relay", "bob")?;

        let decision = manager.route("alice", "bob")?;
        assert_eq!(decision.route.path, ["alice", "fast-relay", "bob"]);
        assert_eq!(decision.ranked_source_neighbors[0], "fast-relay");
        assert!(decision.manager_epoch >= 8);
        Ok(())
    }

    #[test]
    fn updated_observations_change_route_without_rebuilding_graph(
    ) -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("relay-a", 10, 10, 0),
            observation("relay-b", 50, 10, 0),
            observation("bob", 5, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "relay-a")?;
        manager.connect_peers("relay-a", "bob")?;
        manager.connect_peers("alice", "relay-b")?;
        manager.connect_peers("relay-b", "bob")?;
        assert_eq!(
            manager.route("alice", "bob")?.route.path,
            ["alice", "relay-a", "bob"]
        );

        manager.upsert_observation(observation("relay-a", 500, 1, 50_000))?;
        assert_eq!(
            manager.route("alice", "bob")?.route.path,
            ["alice", "relay-b", "bob"]
        );
        Ok(())
    }

    #[test]
    fn manager_accepts_capability_advertisements_as_routing_inputs(
    ) -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for (peer_id, rtt) in [("alice", 5), ("relay-a", 25), ("relay-b", 75), ("bob", 5)] {
            manager.upsert_capability_advertisement(
                RelayCapabilityAdvertisement {
                    peer_id: peer_id.to_owned(),
                    sequence: 1,
                    issued_at_ms: 1_000,
                    expires_at_ms: 2_000,
                    relay_capacity: crate::RelayCapacityAdvertisement {
                        max_fanout: 8,
                        egress_bytes_per_second: 128_000,
                        accepts_store_forward: true,
                    },
                    battery_doze: crate::BatteryDozePosture::BatteryNormal,
                    observed_rtt_ms: rtt,
                    packet_loss_bps: 0,
                    contributed_bytes: 10_000,
                    consumed_bytes: 0,
                },
                1_500,
            )?;
        }
        manager.connect_peers("alice", "relay-a")?;
        manager.connect_peers("relay-a", "bob")?;
        manager.connect_peers("alice", "relay-b")?;
        manager.connect_peers("relay-b", "bob")?;
        assert_eq!(
            manager.route("alice", "bob")?.route.path,
            ["alice", "relay-a", "bob"]
        );
        Ok(())
    }

    #[test]
    fn constructs_text_and_voice_routes_with_hop_and_capacity_checks(
    ) -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for (peer_id, rtt, egress) in [
            ("alice", 5, 64_000),
            ("relay-a", 25, 64_000),
            ("relay-b", 30, 48_000),
            ("bob", 5, 64_000),
        ] {
            manager.upsert_capability_advertisement(
                RelayCapabilityAdvertisement {
                    peer_id: peer_id.to_owned(),
                    sequence: 1,
                    issued_at_ms: 1_000,
                    expires_at_ms: 2_000,
                    relay_capacity: crate::RelayCapacityAdvertisement {
                        max_fanout: 8,
                        egress_bytes_per_second: egress,
                        accepts_store_forward: true,
                    },
                    battery_doze: crate::BatteryDozePosture::BatteryNormal,
                    observed_rtt_ms: rtt,
                    packet_loss_bps: 0,
                    contributed_bytes: 10_000,
                    consumed_bytes: 0,
                },
                1_500,
            )?;
        }
        manager.connect_peers("alice", "relay-a")?;
        manager.connect_peers("relay-a", "relay-b")?;
        manager.connect_peers("relay-b", "bob")?;

        let text = manager.construct_route(OverlayRouteUse::TextControl, "alice", "bob")?;
        assert_eq!(text.hop_count, 3);
        assert_eq!(text.bottleneck_egress_bytes_per_second, Some(48_000));
        let voice = manager.construct_route(OverlayRouteUse::VoiceMedia, "alice", "bob")?;
        assert_eq!(voice.route.path, ["alice", "relay-a", "relay-b", "bob"]);
        Ok(())
    }

    #[test]
    fn voice_route_rejects_under_capacity_relay() -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for (peer_id, egress) in [("alice", 64_000), ("relay", 8_000), ("bob", 64_000)] {
            manager.upsert_capability_advertisement(
                RelayCapabilityAdvertisement {
                    peer_id: peer_id.to_owned(),
                    sequence: 1,
                    issued_at_ms: 1_000,
                    expires_at_ms: 2_000,
                    relay_capacity: crate::RelayCapacityAdvertisement {
                        max_fanout: 8,
                        egress_bytes_per_second: egress,
                        accepts_store_forward: true,
                    },
                    battery_doze: crate::BatteryDozePosture::BatteryNormal,
                    observed_rtt_ms: 20,
                    packet_loss_bps: 0,
                    contributed_bytes: 10_000,
                    consumed_bytes: 0,
                },
                1_500,
            )?;
        }
        manager.connect_peers("alice", "relay")?;
        manager.connect_peers("relay", "bob")?;
        assert!(matches!(
            manager.construct_route(OverlayRouteUse::VoiceMedia, "alice", "bob"),
            Err(OverlayManagerError::RouteCapacity(_))
        ));
        assert!(manager
            .construct_route(OverlayRouteUse::TextControl, "alice", "bob")
            .is_ok());
        Ok(())
    }

    #[test]
    fn planned_reparenting_is_damped_to_one_change_per_window() -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default().with_churn_policy(ChurnDampingPolicy {
            min_planned_change_interval_ms: 30_000,
        });
        for peer in [
            observation("alice", 5, 10, 0),
            observation("relay-a", 20, 10, 0),
            observation("relay-b", 30, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        let first = manager.connect_peers_with_churn_damping(
            "alice",
            "relay-a",
            1_000,
            TopologyChangeReason::PlannedReparent,
        )?;
        assert_eq!(first.next_planned_change_allowed_at_ms, Some(31_000));
        assert!(matches!(
            manager.connect_peers_with_churn_damping(
                "alice",
                "relay-b",
                30_999,
                TopologyChangeReason::PlannedReparent,
            ),
            Err(OverlayManagerError::ChurnDamped {
                next_allowed_at_ms: 31_000
            })
        ));
        assert!(manager
            .connect_peers_with_churn_damping(
                "alice",
                "relay-b",
                31_000,
                TopologyChangeReason::PlannedReparent,
            )
            .is_ok());
        Ok(())
    }

    #[test]
    fn hard_failure_reparent_bypasses_churn_damping() -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("relay-a", 20, 10, 0),
            observation("relay-b", 30, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers_with_churn_damping(
            "alice",
            "relay-a",
            1_000,
            TopologyChangeReason::PlannedReparent,
        )?;
        let hard_failure = manager.connect_peers_with_churn_damping(
            "alice",
            "relay-b",
            1_001,
            TopologyChangeReason::HardFailure,
        )?;
        assert_eq!(hard_failure.reason, TopologyChangeReason::HardFailure);
        Ok(())
    }

    #[test]
    fn failover_report_avoids_failed_peer_and_records_convergence(
    ) -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("primary", 10, 10, 0),
            observation("backup", 30, 10, 0),
            observation("bob", 5, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "primary")?;
        manager.connect_peers("primary", "bob")?;
        manager.connect_peers("alice", "backup")?;
        manager.connect_peers("backup", "bob")?;

        let previous = manager.route("alice", "bob")?.route;
        let report = manager.mark_failed_and_reroute(previous, "primary", 2_400)?;
        assert_eq!(report.decision.replacement.path, ["alice", "backup", "bob"]);
        assert!(report.decision.converged_within_phase2_gate());
        assert_eq!(report.recovery_policy.max_failover_ms, 3_000);
        assert!(report.media_concealment.is_none());
        assert!(!manager
            .route("alice", "bob")?
            .route
            .contains_peer("primary"));
        Ok(())
    }

    #[test]
    fn media_failover_enforces_three_second_and_200ms_gap_targets(
    ) -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("primary", 10, 10, 0),
            observation("backup", 30, 10, 0),
            observation("bob", 5, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "primary")?;
        manager.connect_peers("primary", "bob")?;
        manager.connect_peers("alice", "backup")?;
        manager.connect_peers("backup", "bob")?;

        let previous = manager.route("alice", "bob")?.route;
        let report = manager.mark_failed_media_and_reroute(previous, "primary", 2_750, 180)?;
        assert_eq!(report.decision.replacement.path, ["alice", "backup", "bob"]);
        assert_eq!(
            report.media_concealment,
            Some(MediaConcealmentReport {
                observed_gap_ms: 180,
                target_gap_ms: 200,
                concealment_frames: 9,
                target_met: true,
            })
        );
        Ok(())
    }

    #[test]
    fn failover_rejects_sla_and_media_gap_violations() -> Result<(), OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            observation("alice", 5, 10, 0),
            observation("primary", 10, 10, 0),
            observation("backup", 30, 10, 0),
            observation("bob", 5, 10, 0),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "primary")?;
        manager.connect_peers("primary", "bob")?;
        manager.connect_peers("alice", "backup")?;
        manager.connect_peers("backup", "bob")?;

        let previous = manager.route("alice", "bob")?.route;
        assert!(matches!(
            manager.mark_failed_media_and_reroute(previous.clone(), "primary", 3_001, 180),
            Err(OverlayManagerError::FailoverSlaExceeded {
                observed_ms: 3_001,
                max_failover_ms: 3_000
            })
        ));
        assert!(matches!(
            manager.mark_failed_media_and_reroute(previous, "primary", 2_900, 201),
            Err(OverlayManagerError::MediaGapExceeded {
                observed_gap_ms: 201,
                max_gap_ms: 200
            })
        ));
        Ok(())
    }

    #[test]
    fn rejects_empty_or_unprobed_runtime_observations() {
        let mut manager = OverlayManager::default();
        let mut invalid = observation("", 10, 1, 0);
        assert!(matches!(
            manager.upsert_observation(invalid.clone()),
            Err(OverlayManagerError::InvalidObservation(_))
        ));
        invalid.peer_id = "relay".to_owned();
        invalid.successful_probes = 0;
        invalid.failed_probes = 0;
        assert!(matches!(
            manager.upsert_observation(invalid),
            Err(OverlayManagerError::InvalidObservation(_))
        ));
    }
}
