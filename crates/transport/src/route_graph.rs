//! Per-group/channel route graph model for admitted peer transport intents.
//!
//! The graph is data-only: it records which admitted remote peers need direct
//! or TURN-backed WebRTC routes from the local admitted peer. It does not attach
//! sessions, fan out messages, export diagnostics, or treat signaling providers
//! as application relays.

use crate::{
    ConversationScope, Endpoint, FallbackLeg, RouteReport, SignalingPeerId, TransportError,
    TransportRoute,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Serializable schema version for group route graph snapshots.
pub const ROUTE_GRAPH_SCHEMA_VERSION: u16 = 1;

/// Group/channel scope for route graph edges.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct RouteGraphScope {
    /// Committed group/server scope.
    pub group: ConversationScope,
    /// Committed channel scope inherited from the group.
    pub channel: ConversationScope,
}

impl RouteGraphScope {
    /// Build and validate a route graph scope.
    pub fn new(
        group: ConversationScope,
        channel: ConversationScope,
    ) -> Result<Self, TransportError> {
        let scope = Self { group, channel };
        scope.validate()?;
        Ok(scope)
    }

    /// Validate group/channel levels and parent linkage.
    pub fn validate(&self) -> Result<(), TransportError> {
        self.group.validate()?;
        self.channel.validate()?;
        if self.group.level != crate::ConnectivityScopeLevel::Group {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph group scope must use group level".to_owned(),
            ));
        }
        if self.channel.level != crate::ConnectivityScopeLevel::Channel {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph channel scope must use channel level".to_owned(),
            ));
        }
        if self.channel.parent_scope_commitment.as_deref()
            != Some(self.group.scope_id_commitment.as_str())
        {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph channel scope must be parented by the graph group".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Route intent for one admitted remote peer edge.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RouteIntent {
    /// Direct WebRTC route should be established or is established for this peer.
    #[serde(rename = "direct_webrtc")]
    DirectWebRtc {
        /// Optional route evidence from a session or planner.
        #[serde(default)]
        route_report: Option<RouteReport>,
    },
    /// Configured TURN-backed WebRTC route should be established or is established.
    #[serde(rename = "configured_turn_webrtc")]
    ConfiguredTurnWebRtc {
        /// Configured TURN endpoint selected for this edge.
        turn_endpoint: Endpoint,
        /// Optional route evidence from a session or planner.
        #[serde(default)]
        route_report: Option<RouteReport>,
    },
    /// Admitted peer edge exists, but route negotiation has not produced evidence yet.
    Pending {
        /// Redacted reason safe for status surfaces.
        reason: String,
    },
    /// Admitted peer edge exists, but no valid direct/TURN route is available.
    Unavailable {
        /// Redacted reason safe for status surfaces.
        reason: String,
    },
}

impl RouteIntent {
    /// Pending route intent for admitted peers awaiting per-peer negotiation.
    pub fn pending(reason: impl Into<String>) -> Result<Self, TransportError> {
        let reason = validate_reason(reason.into())?;
        Ok(Self::Pending { reason })
    }

    /// Unavailable route intent for admitted peers without a valid direct/TURN route.
    pub fn unavailable(reason: impl Into<String>) -> Result<Self, TransportError> {
        let reason = validate_reason(reason.into())?;
        Ok(Self::Unavailable { reason })
    }

    /// Direct WebRTC route intent with optional direct-route evidence.
    pub fn direct_webrtc(route_report: Option<RouteReport>) -> Result<Self, TransportError> {
        if let Some(report) = &route_report {
            validate_route_report(report, FallbackLeg::Stun)?;
        }
        Ok(Self::DirectWebRtc { route_report })
    }

    /// Configured TURN-backed WebRTC route intent with optional TURN-route evidence.
    pub fn configured_turn_webrtc(
        turn_endpoint: Endpoint,
        route_report: Option<RouteReport>,
    ) -> Result<Self, TransportError> {
        validate_endpoint(&turn_endpoint, "configured TURN endpoint")?;
        if turn_endpoint.0 == crate::ConnectivityConfig::UNCONFIGURED_TURN_ENDPOINT {
            return Err(TransportError::InvalidConnectivityPolicy(
                "configured TURN route intent requires a configured TURN endpoint".to_owned(),
            ));
        }
        if let Some(report) = &route_report {
            validate_route_report(report, FallbackLeg::Turn)?;
        }
        Ok(Self::ConfiguredTurnWebRtc {
            turn_endpoint,
            route_report,
        })
    }

    /// True only for route intents that can become WebRTC delivery routes.
    #[must_use]
    pub const fn is_connectable(&self) -> bool {
        matches!(
            self,
            Self::DirectWebRtc { .. } | Self::ConfiguredTurnWebRtc { .. }
        )
    }

    /// Return the matching transport route, if this intent has one.
    #[must_use]
    pub const fn transport_route(&self) -> Option<TransportRoute> {
        match self {
            Self::DirectWebRtc { .. } => Some(TransportRoute::Direct),
            Self::ConfiguredTurnWebRtc { .. } => Some(TransportRoute::TurnRelay),
            Self::Pending { .. } | Self::Unavailable { .. } => None,
        }
    }
}

/// Directed edge from the local admitted peer to one admitted remote peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteGraphEdge {
    /// Local admitted peer/device id.
    pub local_peer_id: SignalingPeerId,
    /// Remote admitted peer/device id.
    pub remote_peer_id: SignalingPeerId,
    /// Current intended route state for this edge.
    pub intent: RouteIntent,
}

impl RouteGraphEdge {
    /// Build and validate a directed route graph edge.
    pub fn new(
        local_peer_id: SignalingPeerId,
        remote_peer_id: SignalingPeerId,
        intent: RouteIntent,
    ) -> Result<Self, TransportError> {
        if local_peer_id == remote_peer_id {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph edge cannot target the local peer".to_owned(),
            ));
        }
        Ok(Self {
            local_peer_id,
            remote_peer_id,
            intent,
        })
    }
}

/// Per-group/channel route graph from one local admitted peer to admitted remotes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GroupRouteGraph {
    /// Snapshot schema version.
    pub schema_version: u16,
    /// Group/channel scope for all edges.
    pub scope: RouteGraphScope,
    /// Local admitted peer/device id.
    pub local_peer_id: SignalingPeerId,
    /// Directed edges keyed by remote admitted peer id.
    pub edges: BTreeMap<SignalingPeerId, RouteGraphEdge>,
}

impl GroupRouteGraph {
    /// Build a graph with one edge for every admitted remote peer.
    pub fn new(
        scope: RouteGraphScope,
        local_peer_id: SignalingPeerId,
        admitted_remote_peers: impl IntoIterator<Item = SignalingPeerId>,
        default_intent: RouteIntent,
    ) -> Result<Self, TransportError> {
        scope.validate()?;
        let mut seen = BTreeSet::new();
        let mut edges = BTreeMap::new();
        for remote_peer_id in admitted_remote_peers {
            if remote_peer_id == local_peer_id {
                return Err(TransportError::InvalidConnectivityPolicy(
                    "route graph admitted remotes must not include the local peer".to_owned(),
                ));
            }
            if !seen.insert(remote_peer_id.clone()) {
                return Err(TransportError::InvalidConnectivityPolicy(
                    "route graph admitted remote peers must be unique".to_owned(),
                ));
            }
            let edge = RouteGraphEdge::new(
                local_peer_id.clone(),
                remote_peer_id.clone(),
                default_intent.clone(),
            )?;
            edges.insert(remote_peer_id, edge);
        }
        if edges.is_empty() {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph requires at least one admitted remote peer".to_owned(),
            ));
        }
        Ok(Self {
            schema_version: ROUTE_GRAPH_SCHEMA_VERSION,
            scope,
            local_peer_id,
            edges,
        })
    }

    /// Build the legacy two-person graph shape used by existing pairwise flows.
    pub fn two_person(
        scope: RouteGraphScope,
        local_peer_id: SignalingPeerId,
        remote_peer_id: SignalingPeerId,
        intent: RouteIntent,
    ) -> Result<Self, TransportError> {
        Self::new(scope, local_peer_id, [remote_peer_id], intent)
    }

    /// Number of admitted remote peer edges.
    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }

    /// Return the edge for a remote admitted peer.
    #[must_use]
    pub fn edge_for(&self, remote_peer_id: &SignalingPeerId) -> Option<&RouteGraphEdge> {
        self.edges.get(remote_peer_id)
    }

    /// Update one remote peer edge intent.
    pub fn set_intent(
        &mut self,
        remote_peer_id: &SignalingPeerId,
        intent: RouteIntent,
    ) -> Result<(), TransportError> {
        let edge = self.edges.get_mut(remote_peer_id).ok_or_else(|| {
            TransportError::InvalidConnectivityPolicy(
                "route graph remote peer is not an admitted edge".to_owned(),
            )
        })?;
        edge.intent = intent;
        Ok(())
    }

    /// Validate snapshot invariants after decoding from persisted state.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.schema_version != ROUTE_GRAPH_SCHEMA_VERSION {
            return Err(TransportError::InvalidConnectivityPolicy(
                "unsupported route graph schema version".to_owned(),
            ));
        }
        self.scope.validate()?;
        if self.edges.is_empty() {
            return Err(TransportError::InvalidConnectivityPolicy(
                "route graph requires at least one admitted remote peer".to_owned(),
            ));
        }
        for (remote_peer_id, edge) in &self.edges {
            if remote_peer_id != &edge.remote_peer_id {
                return Err(TransportError::InvalidConnectivityPolicy(
                    "route graph edge key must match remote peer id".to_owned(),
                ));
            }
            if edge.local_peer_id != self.local_peer_id {
                return Err(TransportError::InvalidConnectivityPolicy(
                    "route graph edge local peer must match graph local peer".to_owned(),
                ));
            }
            RouteGraphEdge::new(
                edge.local_peer_id.clone(),
                edge.remote_peer_id.clone(),
                edge.intent.clone(),
            )?;
        }
        Ok(())
    }
}

fn validate_reason(reason: String) -> Result<String, TransportError> {
    if reason.trim().is_empty() || reason.trim() != reason || reason.len() > 256 {
        Err(TransportError::InvalidConnectivityPolicy(
            "route graph reason must be non-empty trimmed text up to 256 bytes".to_owned(),
        ))
    } else {
        Ok(reason)
    }
}

fn validate_endpoint(endpoint: &Endpoint, label: &str) -> Result<(), TransportError> {
    if endpoint.0.trim().is_empty() || endpoint.0.trim() != endpoint.0 {
        Err(TransportError::InvalidConnectivityPolicy(format!(
            "route graph {label} must be non-empty trimmed text"
        )))
    } else {
        Ok(())
    }
}

fn validate_route_report(
    report: &RouteReport,
    expected: FallbackLeg,
) -> Result<(), TransportError> {
    if report.selected != expected {
        return Err(TransportError::InvalidConnectivityPolicy(
            "route graph route report selected leg does not match intent".to_owned(),
        ));
    }
    if !report.honest_and_ordered() {
        return Err(TransportError::InvalidConnectivityPolicy(
            "route graph route report must preserve direct/TURN ordering and ciphertext-only relay policy"
                .to_owned(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{derive_scope_commitment, ConnectivityScopeLevel};

    fn scope() -> Result<RouteGraphScope, TransportError> {
        let group = ConversationScope::new(
            ConnectivityScopeLevel::Group,
            derive_scope_commitment(
                ConnectivityScopeLevel::Group,
                b"per60 group",
                "route-graph-test",
            ),
        )?;
        let channel = ConversationScope::new(
            ConnectivityScopeLevel::Channel,
            derive_scope_commitment(
                ConnectivityScopeLevel::Channel,
                b"per60 channel",
                "route-graph-test",
            ),
        )?
        .with_parent_scope_commitment(group.scope_id_commitment.clone())?;
        RouteGraphScope::new(group, channel)
    }

    fn peer(index: usize) -> Result<SignalingPeerId, TransportError> {
        SignalingPeerId::new(format!("peer-{index:02}"))
    }

    fn remotes(count: usize) -> Result<Vec<SignalingPeerId>, TransportError> {
        (1..count).map(peer).collect()
    }

    fn route_report(selected: FallbackLeg) -> RouteReport {
        let attempted_legs = match selected {
            FallbackLeg::Stun => vec![FallbackLeg::Stun],
            FallbackLeg::Turn => vec![FallbackLeg::Stun, FallbackLeg::Turn],
            FallbackLeg::RelayOverlay => vec![FallbackLeg::RelayOverlay],
        };
        RouteReport {
            selected,
            endpoint: Endpoint::new(match selected {
                FallbackLeg::Stun => "stun:direct.example:3478",
                FallbackLeg::Turn => "turns:relay.example:5349",
                FallbackLeg::RelayOverlay => "overlay:unsupported",
            }),
            attempted_legs,
            ciphertext_only_relay_legs: selected != FallbackLeg::RelayOverlay,
            limitation: "deterministic local-process route graph unit proof".to_owned(),
        }
    }

    #[test]
    fn builds_three_member_route_graph_edges_for_each_admitted_remote() -> Result<(), TransportError>
    {
        let graph = GroupRouteGraph::new(
            scope()?,
            peer(0)?,
            remotes(3)?,
            RouteIntent::pending("awaiting per-peer negotiation")?,
        )?;

        assert_eq!(graph.edge_count(), 2);
        assert_eq!(graph.schema_version, ROUTE_GRAPH_SCHEMA_VERSION);
        for remote in remotes(3)? {
            let edge = graph.edge_for(&remote).expect("remote edge");
            assert_eq!(edge.local_peer_id, peer(0)?);
            assert_eq!(edge.remote_peer_id, remote);
            assert!(!edge.intent.is_connectable());
        }
        graph.validate()?;
        Ok(())
    }

    #[test]
    fn builds_eight_member_route_graph_with_direct_and_turn_intents() -> Result<(), TransportError>
    {
        let mut graph = GroupRouteGraph::new(
            scope()?,
            peer(0)?,
            remotes(8)?,
            RouteIntent::pending("awaiting per-peer negotiation")?,
        )?;

        graph.set_intent(
            &peer(1)?,
            RouteIntent::direct_webrtc(Some(route_report(FallbackLeg::Stun)))?,
        )?;
        graph.set_intent(
            &peer(2)?,
            RouteIntent::configured_turn_webrtc(
                Endpoint::new("turns:relay.example:5349"),
                Some(route_report(FallbackLeg::Turn)),
            )?,
        )?;

        assert_eq!(graph.edge_count(), 7);
        assert_eq!(
            graph
                .edge_for(&peer(1)?)
                .and_then(|edge| edge.intent.transport_route()),
            Some(TransportRoute::Direct)
        );
        assert_eq!(
            graph
                .edge_for(&peer(2)?)
                .and_then(|edge| edge.intent.transport_route()),
            Some(TransportRoute::TurnRelay)
        );
        assert_eq!(
            graph
                .edge_for(&peer(3)?)
                .and_then(|edge| edge.intent.transport_route()),
            None
        );
        graph.validate()?;
        Ok(())
    }

    #[test]
    fn builds_sixteen_member_route_graph_with_unavailable_fail_closed_edges(
    ) -> Result<(), TransportError> {
        let mut graph = GroupRouteGraph::new(
            scope()?,
            peer(0)?,
            remotes(16)?,
            RouteIntent::pending("awaiting per-peer negotiation")?,
        )?;
        graph.set_intent(
            &peer(15)?,
            RouteIntent::unavailable("direct failed and configured TURN is missing")?,
        )?;

        assert_eq!(graph.edge_count(), 15);
        assert_eq!(
            graph
                .edge_for(&peer(15)?)
                .and_then(|edge| edge.intent.transport_route()),
            None
        );
        graph.validate()?;
        Ok(())
    }

    #[test]
    fn two_person_graph_round_trips_as_stable_json() -> Result<(), Box<dyn std::error::Error>> {
        let graph = GroupRouteGraph::two_person(
            scope()?,
            peer(0)?,
            peer(1)?,
            RouteIntent::direct_webrtc(Some(route_report(FallbackLeg::Stun)))?,
        )?;

        let value = serde_json::to_value(&graph)?;
        assert_eq!(
            value.get("schema_version"),
            Some(&serde_json::json!(ROUTE_GRAPH_SCHEMA_VERSION))
        );
        assert_eq!(
            value.pointer("/edges/peer-01/intent/kind"),
            Some(&serde_json::json!("direct_webrtc"))
        );

        let decoded: GroupRouteGraph = serde_json::from_value(value)?;
        decoded.validate()?;
        assert_eq!(decoded, graph);
        Ok(())
    }

    #[test]
    fn rejects_unadmitted_duplicate_local_and_unconfigured_turn_edges() -> Result<(), TransportError>
    {
        let duplicate = GroupRouteGraph::new(
            scope()?,
            peer(0)?,
            [peer(1)?, peer(1)?],
            RouteIntent::pending("awaiting per-peer negotiation")?,
        );
        assert!(duplicate.is_err());

        let local_remote = GroupRouteGraph::new(
            scope()?,
            peer(0)?,
            [peer(0)?],
            RouteIntent::pending("awaiting per-peer negotiation")?,
        );
        assert!(local_remote.is_err());

        let unconfigured_turn = RouteIntent::configured_turn_webrtc(
            Endpoint::new(crate::ConnectivityConfig::UNCONFIGURED_TURN_ENDPOINT),
            None,
        );
        assert!(unconfigured_turn.is_err());
        Ok(())
    }

    #[test]
    fn rejects_provider_or_overlay_route_report_as_application_route() -> Result<(), TransportError>
    {
        let overlay = RouteIntent::direct_webrtc(Some(route_report(FallbackLeg::RelayOverlay)));
        assert!(overlay.is_err());

        let mismatched_turn = RouteIntent::configured_turn_webrtc(
            Endpoint::new("turns:relay.example:5349"),
            Some(route_report(FallbackLeg::Stun)),
        );
        assert!(mismatched_turn.is_err());
        Ok(())
    }
}
