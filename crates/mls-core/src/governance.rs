//! Signed governance event ordering primitives.

use crate::LeafIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;

/// Room role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Role {
    Owner,
    Admin,
    Member,
}

/// Governance action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GovernanceAction {
    SetRole {
        target: LeafIndex,
        role: Role,
    },
    RevokeInvite {
        invite_id: String,
    },
    SetRetentionSeconds {
        author: LeafIndex,
        seconds: Option<u64>,
    },
    Ban {
        target: LeafIndex,
    },
}

/// Signed event placeholder. Phase 0 stores signature bytes; later phases verify with MLS credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceEvent {
    pub epoch: u64,
    pub committer: LeafIndex,
    pub action: GovernanceAction,
    pub signature: Vec<u8>,
}

impl GovernanceEvent {
    /// Content hash used in the canonical comparator.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let encoded =
            serde_json::to_vec(&(self.epoch, self.committer, &self.action)).unwrap_or_default();
        Sha256::digest(encoded).into()
    }

    /// Comparator reference anchored to an accepted tree.
    #[must_use]
    pub fn canonical_ref(&self) -> CanonicalEventRef {
        CanonicalEventRef {
            epoch: self.epoch,
            committer: self.committer,
            content_hash: self.content_hash(),
        }
    }
}

/// Canonical event reference: epoch → committer leaf → signed content hash.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CanonicalEventRef {
    pub epoch: u64,
    pub committer: LeafIndex,
    pub content_hash: [u8; 32],
}

impl Ord for CanonicalEventRef {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.epoch, self.committer, self.content_hash).cmp(&(
            other.epoch,
            other.committer,
            other.content_hash,
        ))
    }
}
impl PartialOrd for CanonicalEventRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Ordered governance log.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GovernanceLog {
    events: Vec<GovernanceEvent>,
}

impl GovernanceLog {
    /// Insert then sort by canonical comparator.
    pub fn append(&mut self, event: GovernanceEvent) {
        self.events.push(event);
        self.events.sort_by_key(GovernanceEvent::canonical_ref);
    }

    /// Ordered events.
    #[must_use]
    pub fn events(&self) -> &[GovernanceEvent] {
        &self.events
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_order_is_deterministic() {
        let mut log = GovernanceLog::default();
        let a = GovernanceEvent {
            epoch: 1,
            committer: 2,
            action: GovernanceAction::Ban { target: 9 },
            signature: vec![],
        };
        let b = GovernanceEvent {
            epoch: 1,
            committer: 1,
            action: GovernanceAction::Ban { target: 8 },
            signature: vec![],
        };
        log.append(a);
        log.append(b);
        assert_eq!(log.events()[0].committer, 1);
    }
}
