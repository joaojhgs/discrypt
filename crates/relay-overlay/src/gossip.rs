//! Deterministic gossip convergence primitives for text/history sync.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Relay-gossiped author-log item. Payload stays ciphertext/content-blind.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct GossipItem {
    /// Author MLS leaf.
    pub author_leaf: u32,
    /// Author-log sequence.
    pub sequence: u64,
    /// Stable message id.
    pub message_id: String,
    /// Ciphertext hash; relays do not inspect plaintext.
    pub ciphertext_hash: [u8; 32],
}

impl GossipItem {
    /// Build a gossip item from ciphertext bytes.
    #[must_use]
    pub fn new(
        author_leaf: u32,
        sequence: u64,
        message_id: impl Into<String>,
        ciphertext: &[u8],
    ) -> Self {
        Self {
            author_leaf,
            sequence,
            message_id: message_id.into(),
            ciphertext_hash: Sha256::digest(ciphertext).into(),
        }
    }
}

/// Local gossip peer state.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct GossipNode {
    items: BTreeSet<GossipItem>,
}

impl GossipNode {
    /// Insert a content-blind item.
    pub fn insert(&mut self, item: GossipItem) {
        self.items.insert(item);
    }

    /// Merge items from another peer.
    pub fn merge<I>(&mut self, items: I) -> usize
    where
        I: IntoIterator<Item = GossipItem>,
    {
        let mut inserted = 0;
        for item in items {
            if self.items.insert(item) {
                inserted += 1;
            }
        }
        inserted
    }

    /// Snapshot known items.
    #[must_use]
    pub fn snapshot(&self) -> Vec<GossipItem> {
        self.items.iter().cloned().collect()
    }

    /// Known item count.
    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// True when no items are known.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Deterministic full-fanout gossip mesh for headless harnesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GossipMesh {
    nodes: BTreeMap<String, GossipNode>,
}

impl GossipMesh {
    /// Create a mesh with named peers.
    #[must_use]
    pub fn new<I, S>(peer_ids: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            nodes: peer_ids
                .into_iter()
                .map(|peer| (peer.into(), GossipNode::default()))
                .collect(),
        }
    }

    /// Insert an item into a peer.
    pub fn insert(&mut self, peer_id: &str, item: GossipItem) {
        if let Some(node) = self.nodes.get_mut(peer_id) {
            node.insert(item);
        }
    }

    /// Run one deterministic gossip round by unioning all peer item sets.
    pub fn round(&mut self) -> usize {
        let union: Vec<GossipItem> = self
            .nodes
            .values()
            .flat_map(GossipNode::snapshot)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        self.nodes
            .values_mut()
            .map(|node| node.merge(union.clone()))
            .sum()
    }

    /// True when every node has the same item set.
    #[must_use]
    pub fn converged(&self) -> bool {
        let mut snapshots = self.nodes.values().map(GossipNode::snapshot);
        let Some(first) = snapshots.next() else {
            return true;
        };
        snapshots.all(|snapshot| snapshot == first)
    }

    /// Known count for a peer.
    #[must_use]
    pub fn known_count(&self, peer_id: &str) -> Option<usize> {
        self.nodes.get(peer_id).map(GossipNode::len)
    }

    /// Snapshot for a peer.
    #[must_use]
    pub fn snapshot(&self, peer_id: &str) -> Vec<GossipItem> {
        self.nodes
            .get(peer_id)
            .map_or_else(Vec::new, GossipNode::snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converges_author_log_items_across_sixteen_peers() {
        let peers = (0..16).map(|idx| format!("peer-{idx}")).collect::<Vec<_>>();
        let mut mesh = GossipMesh::new(peers.clone());
        for (idx, peer) in peers.iter().enumerate() {
            mesh.insert(
                peer,
                GossipItem::new(idx as u32, 1, format!("m-{idx}"), b"ciphertext"),
            );
        }
        assert!(!mesh.converged());
        assert!(mesh.round() > 0);
        assert!(mesh.converged());
        assert_eq!(mesh.known_count("peer-0"), Some(16));
    }
}
