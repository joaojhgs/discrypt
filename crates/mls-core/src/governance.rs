//! Signed governance event ordering and authority primitives.

use crate::LeafIndex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Room role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum Role {
    /// Room owner.
    Owner,
    /// Room administrator.
    Admin,
    /// Normal member.
    Member,
}

/// Governance action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum GovernanceAction {
    /// Set a member role.
    SetRole { target: LeafIndex, role: Role },
    /// Revoke an invite.
    RevokeInvite { invite_id: String },
    /// Set author retention.
    SetRetentionSeconds {
        author: LeafIndex,
        seconds: Option<u64>,
    },
    /// Ban/evict a member.
    Ban { target: LeafIndex },
}

/// Governance errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GovernanceError {
    /// Signature is absent/invalid in the deterministic facade.
    #[error("invalid governance signature")]
    InvalidSignature,
    /// Event epoch does not match the accepted epoch.
    #[error("out of epoch governance event")]
    OutOfEpoch,
    /// Committer does not have authority for the action.
    #[error("unauthorized governance action")]
    Unauthorized,
    /// Committer was removed at or before the resolved epoch.
    #[error("evicted committer cannot win governance race")]
    EvictedCommitter,
}

/// Signed event placeholder. Phase facades verify deterministic signature bytes;
/// production wiring replaces this with MLS credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceEvent {
    /// Event epoch.
    pub epoch: u64,
    /// Committer leaf.
    pub committer: LeafIndex,
    /// Governance action.
    pub action: GovernanceAction,
    /// Deterministic signature bytes.
    pub signature: Vec<u8>,
}

impl GovernanceEvent {
    /// Construct and deterministically sign an event for harnesses.
    #[must_use]
    pub fn signed(epoch: u64, committer: LeafIndex, action: GovernanceAction) -> Self {
        let mut event = Self {
            epoch,
            committer,
            action,
            signature: Vec::new(),
        };
        event.signature = event.content_hash().to_vec();
        event
    }

    /// Content hash used in the canonical comparator.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let encoded =
            serde_json::to_vec(&(self.epoch, self.committer, &self.action)).unwrap_or_default();
        Sha256::digest(encoded).into()
    }

    /// Validate deterministic signature placeholder.
    #[must_use]
    pub fn signature_valid(&self) -> bool {
        self.signature == self.content_hash()
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
    /// Event epoch.
    pub epoch: u64,
    /// Committer leaf.
    pub committer: LeafIndex,
    /// Signed content hash.
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

/// Resolved governance room state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceState {
    /// Current accepted epoch.
    pub epoch: u64,
    roles: BTreeMap<LeafIndex, Role>,
    banned: BTreeSet<LeafIndex>,
    revoked_invites: BTreeSet<String>,
    retention_seconds: BTreeMap<LeafIndex, Option<u64>>,
}

impl GovernanceState {
    /// Create a state with one owner.
    #[must_use]
    pub fn new(epoch: u64, owner: LeafIndex) -> Self {
        Self {
            epoch,
            roles: BTreeMap::from([(owner, Role::Owner)]),
            banned: BTreeSet::new(),
            revoked_invites: BTreeSet::new(),
            retention_seconds: BTreeMap::new(),
        }
    }

    /// Role lookup.
    #[must_use]
    pub fn role(&self, leaf: LeafIndex) -> Option<Role> {
        self.roles.get(&leaf).copied()
    }

    /// True when leaf is banned/evicted.
    #[must_use]
    pub fn is_banned(&self, leaf: LeafIndex) -> bool {
        self.banned.contains(&leaf)
    }

    /// True when invite is revoked.
    #[must_use]
    pub fn invite_revoked(&self, invite_id: &str) -> bool {
        self.revoked_invites.contains(invite_id)
    }

    /// Apply one event after validation.
    pub fn apply_event(&mut self, event: GovernanceEvent) -> Result<(), GovernanceError> {
        self.apply_event_with_evicted_set(event, &BTreeSet::new())
    }

    /// Resolve a same-epoch batch with canonical ordering and removed-admin protection.
    pub fn resolve_epoch_events<I>(&mut self, events: I) -> Vec<Result<(), GovernanceError>>
    where
        I: IntoIterator<Item = GovernanceEvent>,
    {
        let mut events = events.into_iter().collect::<Vec<_>>();
        events.sort_by_key(GovernanceEvent::canonical_ref);
        let evicted_this_epoch = events
            .iter()
            .filter_map(|event| match event.action {
                GovernanceAction::Ban { target } if event.epoch == self.epoch => Some(target),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        events
            .into_iter()
            .map(|event| self.apply_event_with_evicted_set(event, &evicted_this_epoch))
            .collect()
    }

    fn apply_event_with_evicted_set(
        &mut self,
        event: GovernanceEvent,
        evicted_this_epoch: &BTreeSet<LeafIndex>,
    ) -> Result<(), GovernanceError> {
        if !event.signature_valid() {
            return Err(GovernanceError::InvalidSignature);
        }
        if event.epoch != self.epoch {
            return Err(GovernanceError::OutOfEpoch);
        }
        if self.banned.contains(&event.committer) || evicted_this_epoch.contains(&event.committer) {
            return Err(GovernanceError::EvictedCommitter);
        }
        if !self.authorized(event.committer, &event.action) {
            return Err(GovernanceError::Unauthorized);
        }
        match event.action {
            GovernanceAction::SetRole { target, role } => {
                self.roles.insert(target, role);
            }
            GovernanceAction::RevokeInvite { invite_id } => {
                self.revoked_invites.insert(invite_id);
            }
            GovernanceAction::SetRetentionSeconds { author, seconds } => {
                self.retention_seconds.insert(author, seconds);
            }
            GovernanceAction::Ban { target } => {
                self.banned.insert(target);
                self.roles.remove(&target);
            }
        }
        Ok(())
    }

    fn authorized(&self, committer: LeafIndex, action: &GovernanceAction) -> bool {
        match (self.role(committer), action) {
            (Some(Role::Owner), _) => true,
            (Some(Role::Admin), GovernanceAction::SetRole { .. }) => false,
            (Some(Role::Admin), _) => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_order_is_deterministic() {
        let mut log = GovernanceLog::default();
        let a = GovernanceEvent::signed(1, 2, GovernanceAction::Ban { target: 9 });
        let b = GovernanceEvent::signed(1, 1, GovernanceAction::Ban { target: 8 });
        log.append(a);
        log.append(b);
        assert_eq!(log.events()[0].committer, 1);
    }

    #[test]
    fn rejects_unauthorized_out_of_epoch_and_removed_admin_race() {
        let mut state = GovernanceState::new(4, 1);
        assert_eq!(
            state.apply_event(GovernanceEvent::signed(
                4,
                2,
                GovernanceAction::RevokeInvite {
                    invite_id: "i".into()
                }
            )),
            Err(GovernanceError::Unauthorized)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed(
                5,
                1,
                GovernanceAction::RevokeInvite {
                    invite_id: "i".into()
                }
            )),
            Err(GovernanceError::OutOfEpoch)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed(
                4,
                1,
                GovernanceAction::SetRole {
                    target: 2,
                    role: Role::Admin,
                }
            )),
            Ok(())
        );
        let results = state.resolve_epoch_events([
            GovernanceEvent::signed(
                4,
                2,
                GovernanceAction::RevokeInvite {
                    invite_id: "should-not-win".into(),
                },
            ),
            GovernanceEvent::signed(4, 1, GovernanceAction::Ban { target: 2 }),
        ]);
        assert_eq!(results[0], Ok(()));
        assert_eq!(results[1], Err(GovernanceError::EvictedCommitter));
        assert!(state.is_banned(2));
        assert!(!state.invite_revoked("should-not-win"));
    }
}
