//! Deterministic relay topology with hop and fanout limits.

use crate::ranking::{rank, RelayMetrics};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use thiserror::Error;

/// Phase-2 overlay hop cap from the product plan.
pub const MAX_RELAY_HOPS: usize = 3;
/// Default per-peer relay fanout/capacity target for voice groups larger than mesh.
pub const DEFAULT_MAX_FANOUT: usize = 8;

/// A selected content-blind route through the overlay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayRoute {
    /// Ordered peer ids, including source and destination.
    pub path: Vec<String>,
}

impl RelayRoute {
    /// Number of overlay edges in this route.
    #[must_use]
    pub fn hop_count(&self) -> usize {
        self.path.len().saturating_sub(1)
    }

    /// True when this route respects the Phase-2 hop cap.
    #[must_use]
    pub fn within_hop_limit(&self) -> bool {
        hop_limit_ok(self.hop_count())
    }

    /// True when the route traverses the given peer.
    #[must_use]
    pub fn contains_peer(&self, peer_id: &str) -> bool {
        self.path.iter().any(|peer| peer == peer_id)
    }
}

/// Relay topology failures.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TopologyError {
    /// A requested peer is not part of the topology.
    #[error("unknown peer: {0}")]
    UnknownPeer(String),
    /// Adding a link would exceed the deterministic fanout limit.
    #[error("fanout limit exceeded for peer: {0}")]
    FanoutLimit(String),
    /// No route exists inside the configured hop cap.
    #[error("no route within hop limit")]
    NoRoute,
}

/// Local topology view used by the harness and relay selection.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RelayTopology {
    max_hops: usize,
    max_fanout: usize,
    metrics: BTreeMap<String, RelayMetrics>,
    links: BTreeMap<String, BTreeSet<String>>,
}

impl Default for RelayTopology {
    fn default() -> Self {
        Self::new(MAX_RELAY_HOPS, DEFAULT_MAX_FANOUT)
    }
}

impl RelayTopology {
    /// Create an empty topology.
    #[must_use]
    pub fn new(max_hops: usize, max_fanout: usize) -> Self {
        Self {
            max_hops,
            max_fanout: max_fanout.max(1),
            metrics: BTreeMap::new(),
            links: BTreeMap::new(),
        }
    }

    /// Add or replace peer metrics.
    pub fn upsert_peer(&mut self, metrics: RelayMetrics) {
        self.links.entry(metrics.peer_id.clone()).or_default();
        self.metrics.insert(metrics.peer_id.clone(), metrics);
    }

    /// Connect two peers if both remain under the fanout cap.
    pub fn connect(&mut self, a: &str, b: &str) -> Result<(), TopologyError> {
        self.ensure_peer(a)?;
        self.ensure_peer(b)?;
        self.ensure_fanout(a, b)?;
        self.ensure_fanout(b, a)?;
        self.links
            .entry(a.to_owned())
            .or_default()
            .insert(b.to_owned());
        self.links
            .entry(b.to_owned())
            .or_default()
            .insert(a.to_owned());
        Ok(())
    }

    /// Number of currently connected neighbors for a peer.
    #[must_use]
    pub fn peer_degree(&self, peer_id: &str) -> Option<usize> {
        self.links.get(peer_id).map(BTreeSet::len)
    }

    /// Return ranked neighbors for deterministic relay selection.
    #[must_use]
    pub fn ranked_neighbors(&self, peer_id: &str) -> Vec<RelayMetrics> {
        let Some(neighbors) = self.links.get(peer_id) else {
            return Vec::new();
        };
        rank(
            neighbors
                .iter()
                .filter_map(|neighbor| self.metrics.get(neighbor).cloned())
                .collect(),
        )
    }

    /// Select a route within the hop cap.
    pub fn route(&self, source: &str, destination: &str) -> Result<RelayRoute, TopologyError> {
        self.route_avoiding(source, destination, &BTreeSet::new())
    }

    /// Select a route while avoiding failed peers.
    pub fn route_avoiding(
        &self,
        source: &str,
        destination: &str,
        avoided_peers: &BTreeSet<String>,
    ) -> Result<RelayRoute, TopologyError> {
        self.ensure_peer(source)?;
        self.ensure_peer(destination)?;
        if avoided_peers.contains(source) || avoided_peers.contains(destination) {
            return Err(TopologyError::NoRoute);
        }
        if source == destination {
            return Ok(RelayRoute {
                path: vec![source.to_owned()],
            });
        }

        let mut queue = VecDeque::from([vec![source.to_owned()]]);
        let mut visited = BTreeSet::from([source.to_owned()]);

        while let Some(path) = queue.pop_front() {
            let Some(last) = path.last() else {
                continue;
            };
            if path.len().saturating_sub(1) >= self.max_hops {
                continue;
            }
            for neighbor in self.ranked_neighbors(last) {
                if avoided_peers.contains(&neighbor.peer_id) || visited.contains(&neighbor.peer_id)
                {
                    continue;
                }
                let mut next_path = path.clone();
                next_path.push(neighbor.peer_id.clone());
                if neighbor.peer_id == destination {
                    return Ok(RelayRoute { path: next_path });
                }
                visited.insert(neighbor.peer_id);
                queue.push_back(next_path);
            }
        }

        Err(TopologyError::NoRoute)
    }

    fn ensure_peer(&self, peer_id: &str) -> Result<(), TopologyError> {
        if self.metrics.contains_key(peer_id) {
            Ok(())
        } else {
            Err(TopologyError::UnknownPeer(peer_id.to_owned()))
        }
    }

    fn ensure_fanout(&self, peer_id: &str, other: &str) -> Result<(), TopologyError> {
        let current = self.links.get(peer_id).map_or(0, BTreeSet::len);
        let already_connected = self
            .links
            .get(peer_id)
            .is_some_and(|neighbors| neighbors.contains(other));
        if already_connected || current < self.max_fanout {
            Ok(())
        } else {
            Err(TopologyError::FanoutLimit(peer_id.to_owned()))
        }
    }
}

/// True when the hop count respects the Phase-2 cap.
#[must_use]
pub fn hop_limit_ok(hops: usize) -> bool {
    hops <= MAX_RELAY_HOPS
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn route_prefers_ranked_neighbors_and_respects_hop_limit() -> Result<(), TopologyError> {
        let mut topology = RelayTopology::default();
        for peer in [
            metrics("alice", 1),
            metrics("slow-relay", 200),
            metrics("fast-relay", 20),
            metrics("bob", 1),
        ] {
            topology.upsert_peer(peer);
        }
        topology.connect("alice", "slow-relay")?;
        topology.connect("alice", "fast-relay")?;
        topology.connect("slow-relay", "bob")?;
        topology.connect("fast-relay", "bob")?;

        let route = topology.route("alice", "bob")?;
        assert_eq!(route.path, ["alice", "fast-relay", "bob"]);
        assert!(route.within_hop_limit());
        assert!(!hop_limit_ok(4));
        Ok(())
    }

    #[test]
    fn refuses_routes_deeper_than_three_hops() -> Result<(), TopologyError> {
        let mut topology = RelayTopology::default();
        for peer in ["a", "b", "c", "d", "e"] {
            topology.upsert_peer(metrics(peer, 10));
        }
        topology.connect("a", "b")?;
        topology.connect("b", "c")?;
        topology.connect("c", "d")?;
        topology.connect("d", "e")?;

        assert_eq!(topology.route("a", "e"), Err(TopologyError::NoRoute));
        Ok(())
    }
}
