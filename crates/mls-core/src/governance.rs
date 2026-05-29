//! Signed governance event ordering and authority primitives.

use crate::LeafIndex;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use rand_core::OsRng;
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
    /// Signature is absent, malformed, or does not verify over the canonical event.
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

/// Governance event signed by a concrete device credential.
///
/// The signature covers a domain-separated canonical payload containing the epoch,
/// committer leaf, signer device public key, and action. The signature bytes are no
/// longer a deterministic content-hash facade; tampering with any signed field or
/// swapping the signer key invalidates the event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GovernanceEvent {
    /// Event epoch.
    pub epoch: u64,
    /// Committer leaf.
    pub committer: LeafIndex,
    /// Governance action.
    pub action: GovernanceAction,
    /// Ed25519 device verification key that signed this event.
    pub signer_public_key: Vec<u8>,
    /// Ed25519 signature over the canonical event payload.
    pub signature: Vec<u8>,
}

impl GovernanceEvent {
    /// Construct a governance event signed by the supplied device signing key.
    #[must_use]
    pub fn signed_by(
        epoch: u64,
        committer: LeafIndex,
        action: GovernanceAction,
        signing_key: &SigningKey,
    ) -> Self {
        let signer_public_key = signing_key.verifying_key().to_bytes().to_vec();
        let signature = signing_key
            .sign(&Self::canonical_signing_bytes(
                epoch,
                committer,
                &action,
                &signer_public_key,
            ))
            .to_bytes()
            .to_vec();
        Self {
            epoch,
            committer,
            action,
            signer_public_key,
            signature,
        }
    }

    /// Construct a real-signed event with a throwaway key for harness scenarios that
    /// only exercise governance ordering/authority. Production call sites should use
    /// [`Self::signed_by`] with the local device key so peers can bind the signer to
    /// a device leaf.
    #[must_use]
    pub fn signed(epoch: u64, committer: LeafIndex, action: GovernanceAction) -> Self {
        let signing_key = SigningKey::generate(&mut OsRng);
        Self::signed_by(epoch, committer, action, &signing_key)
    }

    /// Content hash used in the canonical comparator.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        Sha256::digest(self.signing_bytes()).into()
    }

    /// Verify the Ed25519 signature over the canonical governance event payload.
    pub fn verify_signature(&self) -> Result<(), GovernanceError> {
        let verifying_key = VerifyingKey::from_bytes(
            &self
                .signer_public_key
                .as_slice()
                .try_into()
                .map_err(|_| GovernanceError::InvalidSignature)?,
        )
        .map_err(|_| GovernanceError::InvalidSignature)?;
        let signature = Signature::from_slice(&self.signature)
            .map_err(|_| GovernanceError::InvalidSignature)?;
        verifying_key
            .verify(&self.signing_bytes(), &signature)
            .map_err(|_| GovernanceError::InvalidSignature)
    }

    /// Validate the real device signature.
    #[must_use]
    pub fn signature_valid(&self) -> bool {
        self.verify_signature().is_ok()
    }

    fn signing_bytes(&self) -> Vec<u8> {
        Self::canonical_signing_bytes(
            self.epoch,
            self.committer,
            &self.action,
            &self.signer_public_key,
        )
    }

    fn canonical_signing_bytes(
        epoch: u64,
        committer: LeafIndex,
        action: &GovernanceAction,
        signer_public_key: &[u8],
    ) -> Vec<u8> {
        let mut bytes = b"discrypt-governance-event".to_vec();
        bytes.push(1);
        bytes.extend_from_slice(&epoch.to_le_bytes());
        bytes.extend_from_slice(&committer.to_le_bytes());
        bytes.extend_from_slice(&(signer_public_key.len() as u64).to_le_bytes());
        bytes.extend_from_slice(signer_public_key);
        append_governance_action_bytes(&mut bytes, action);
        bytes
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

fn append_governance_action_bytes(bytes: &mut Vec<u8>, action: &GovernanceAction) {
    match action {
        GovernanceAction::SetRole { target, role } => {
            bytes.push(0);
            bytes.extend_from_slice(&target.to_le_bytes());
            bytes.push(match role {
                Role::Owner => 0,
                Role::Admin => 1,
                Role::Member => 2,
            });
        }
        GovernanceAction::RevokeInvite { invite_id } => {
            bytes.push(1);
            bytes.extend_from_slice(&(invite_id.len() as u64).to_le_bytes());
            bytes.extend_from_slice(invite_id.as_bytes());
        }
        GovernanceAction::SetRetentionSeconds { author, seconds } => {
            bytes.push(2);
            bytes.extend_from_slice(&author.to_le_bytes());
            match seconds {
                Some(seconds) => {
                    bytes.push(1);
                    bytes.extend_from_slice(&seconds.to_le_bytes());
                }
                None => bytes.push(0),
            }
        }
        GovernanceAction::Ban { target } => {
            bytes.push(3);
            bytes.extend_from_slice(&target.to_le_bytes());
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
        event.verify_signature()?;
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
    fn real_device_signature_rejects_tampering_and_key_swaps() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let other_key = SigningKey::generate(&mut OsRng);
        let mut event = GovernanceEvent::signed_by(
            7,
            1,
            GovernanceAction::RevokeInvite {
                invite_id: "invite-a".into(),
            },
            &signing_key,
        );

        assert!(event.signature_valid());

        let mut tampered_action = event.clone();
        tampered_action.action = GovernanceAction::RevokeInvite {
            invite_id: "invite-b".into(),
        };
        assert_eq!(
            tampered_action.verify_signature(),
            Err(GovernanceError::InvalidSignature)
        );

        event.signer_public_key = other_key.verifying_key().to_bytes().to_vec();
        assert_eq!(
            event.verify_signature(),
            Err(GovernanceError::InvalidSignature)
        );
    }

    #[test]
    fn rejects_malformed_governance_signature_material() {
        let signing_key = SigningKey::generate(&mut OsRng);
        let mut event = GovernanceEvent::signed_by(
            1,
            1,
            GovernanceAction::SetRetentionSeconds {
                author: 1,
                seconds: Some(60),
            },
            &signing_key,
        );
        event.signature.truncate(8);
        assert_eq!(
            event.verify_signature(),
            Err(GovernanceError::InvalidSignature)
        );

        let mut event =
            GovernanceEvent::signed_by(1, 1, GovernanceAction::Ban { target: 2 }, &signing_key);
        event.signer_public_key.truncate(8);
        assert_eq!(
            event.verify_signature(),
            Err(GovernanceError::InvalidSignature)
        );
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
