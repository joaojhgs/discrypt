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
    /// Add an active device id under a member leaf.
    AddDevice { owner: LeafIndex, device_id: String },
    /// Remove an active device id under a member leaf.
    RemoveDevice { owner: LeafIndex, device_id: String },
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
/// longer a deterministic content-hash substitute; tampering with any signed field or
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
        GovernanceAction::AddDevice { owner, device_id } => {
            bytes.push(4);
            bytes.extend_from_slice(&owner.to_le_bytes());
            bytes.extend_from_slice(&(device_id.len() as u64).to_le_bytes());
            bytes.extend_from_slice(device_id.as_bytes());
        }
        GovernanceAction::RemoveDevice { owner, device_id } => {
            bytes.push(5);
            bytes.extend_from_slice(&owner.to_le_bytes());
            bytes.extend_from_slice(&(device_id.len() as u64).to_le_bytes());
            bytes.extend_from_slice(device_id.as_bytes());
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

/// Order governance events with the single Discrypt application-event comparator:
/// epoch → committer/author leaf index in the last common accepted tree → signed content hash.
#[must_use]
pub fn order_governance_events<I>(events: I) -> Vec<GovernanceEvent>
where
    I: IntoIterator<Item = GovernanceEvent>,
{
    let mut events = events.into_iter().collect::<Vec<_>>();
    events.sort_by_key(GovernanceEvent::canonical_ref);
    events
}

/// Ordered governance log.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct GovernanceLog {
    events: Vec<GovernanceEvent>,
}

impl GovernanceLog {
    /// Insert then sort by the canonical last-common-tree comparator.
    pub fn append(&mut self, event: GovernanceEvent) {
        self.events = order_governance_events(self.events.drain(..).chain([event]));
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
    member_devices: BTreeMap<LeafIndex, BTreeSet<String>>,
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
            member_devices: BTreeMap::new(),
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

    /// Effective per-author retention override recorded by governance.
    #[must_use]
    pub fn retention_seconds(&self, author: LeafIndex) -> Option<Option<u64>> {
        self.retention_seconds.get(&author).copied()
    }

    /// True when the device id is currently active for the member leaf.
    #[must_use]
    pub fn device_active(&self, owner: LeafIndex, device_id: &str) -> bool {
        self.member_devices
            .get(&owner)
            .is_some_and(|devices| devices.contains(device_id))
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
        let events = order_governance_events(events);
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
                self.member_devices.remove(&target);
            }
            GovernanceAction::AddDevice { owner, device_id } => {
                self.member_devices
                    .entry(owner)
                    .or_default()
                    .insert(device_id);
            }
            GovernanceAction::RemoveDevice { owner, device_id } => {
                if let Some(devices) = self.member_devices.get_mut(&owner) {
                    devices.remove(&device_id);
                }
            }
        }
        Ok(())
    }

    fn authorized(&self, committer: LeafIndex, action: &GovernanceAction) -> bool {
        if self.banned.contains(&target_leaf(action)) {
            return false;
        }
        match self.role(committer) {
            Some(Role::Owner) => true,
            Some(Role::Admin) => self.admin_authorized(committer, action),
            Some(Role::Member) => self.member_authorized(committer, action),
            None => false,
        }
    }

    fn admin_authorized(&self, committer: LeafIndex, action: &GovernanceAction) -> bool {
        match action {
            GovernanceAction::SetRole { .. } => false,
            GovernanceAction::RevokeInvite { .. } => true,
            GovernanceAction::SetRetentionSeconds { author, .. } => {
                *author == committer || self.role(*author) == Some(Role::Member)
            }
            GovernanceAction::Ban { target } => self.role(*target) == Some(Role::Member),
            GovernanceAction::AddDevice { owner, .. }
            | GovernanceAction::RemoveDevice { owner, .. } => {
                *owner == committer || self.role(*owner) == Some(Role::Member)
            }
        }
    }

    fn member_authorized(&self, committer: LeafIndex, action: &GovernanceAction) -> bool {
        match action {
            GovernanceAction::SetRetentionSeconds { author, .. } => *author == committer,
            GovernanceAction::AddDevice { owner, .. }
            | GovernanceAction::RemoveDevice { owner, .. } => *owner == committer,
            GovernanceAction::SetRole { .. }
            | GovernanceAction::RevokeInvite { .. }
            | GovernanceAction::Ban { .. } => false,
        }
    }
}

fn target_leaf(action: &GovernanceAction) -> LeafIndex {
    match action {
        GovernanceAction::SetRole { target, .. } | GovernanceAction::Ban { target } => *target,
        GovernanceAction::SetRetentionSeconds { author, .. }
        | GovernanceAction::AddDevice { owner: author, .. }
        | GovernanceAction::RemoveDevice { owner: author, .. } => *author,
        GovernanceAction::RevokeInvite { .. } => LeafIndex::MAX,
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
    fn canonical_order_uses_epoch_last_common_leaf_then_signed_content_hash() {
        let signer = SigningKey::generate(&mut OsRng);
        let first_by_hash = GovernanceEvent::signed_by(
            8,
            5,
            GovernanceAction::RevokeInvite {
                invite_id: "hash-a".into(),
            },
            &signer,
        );
        let second_by_hash = GovernanceEvent::signed_by(
            8,
            5,
            GovernanceAction::RevokeInvite {
                invite_id: "hash-b".into(),
            },
            &signer,
        );
        let lower_leaf_later_arrival = GovernanceEvent::signed_by(
            8,
            2,
            GovernanceAction::SetRetentionSeconds {
                author: 2,
                seconds: Some(120),
            },
            &signer,
        );
        let future_epoch =
            GovernanceEvent::signed_by(9, 1, GovernanceAction::Ban { target: 7 }, &signer);

        let mut expected_same_leaf = [first_by_hash.clone(), second_by_hash.clone()];
        expected_same_leaf.sort_by_key(GovernanceEvent::canonical_ref);

        let ordered = order_governance_events([
            future_epoch.clone(),
            second_by_hash.clone(),
            lower_leaf_later_arrival.clone(),
            first_by_hash.clone(),
        ]);

        assert_eq!(ordered[0], lower_leaf_later_arrival);
        assert_eq!(ordered[1], expected_same_leaf[0]);
        assert_eq!(ordered[2], expected_same_leaf[1]);
        assert_eq!(ordered[3], future_epoch);
        assert!(ordered
            .windows(2)
            .all(|pair| pair[0].canonical_ref() <= pair[1].canonical_ref()));
    }

    #[test]
    fn canonical_resolution_returns_results_in_comparator_order() {
        let owner = SigningKey::generate(&mut OsRng);
        let admin = SigningKey::generate(&mut OsRng);
        let mut state = GovernanceState::new(10, 1);
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                10,
                1,
                GovernanceAction::SetRole {
                    target: 2,
                    role: Role::Admin,
                },
                &owner,
            )),
            Ok(())
        );

        let results = state.resolve_epoch_events([
            GovernanceEvent::signed_by(
                10,
                2,
                GovernanceAction::RevokeInvite {
                    invite_id: "admin-arrived-first".into(),
                },
                &admin,
            ),
            GovernanceEvent::signed_by(10, 1, GovernanceAction::Ban { target: 2 }, &owner),
        ]);

        assert_eq!(
            results,
            vec![Ok(()), Err(GovernanceError::EvictedCommitter)]
        );
        assert!(state.is_banned(2));
        assert!(!state.invite_revoked("admin-arrived-first"));
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
    fn enforces_role_retention_invite_ban_and_device_authority() {
        let owner = SigningKey::generate(&mut OsRng);
        let admin = SigningKey::generate(&mut OsRng);
        let member = SigningKey::generate(&mut OsRng);
        let mut state = GovernanceState::new(12, 1);

        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                1,
                GovernanceAction::SetRole {
                    target: 2,
                    role: Role::Admin,
                },
                &owner,
            )),
            Ok(())
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                1,
                GovernanceAction::SetRole {
                    target: 3,
                    role: Role::Member,
                },
                &owner,
            )),
            Ok(())
        );

        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                3,
                GovernanceAction::RevokeInvite {
                    invite_id: "member-nope".into(),
                },
                &member,
            )),
            Err(GovernanceError::Unauthorized)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                2,
                GovernanceAction::SetRole {
                    target: 3,
                    role: Role::Admin,
                },
                &admin,
            )),
            Err(GovernanceError::Unauthorized)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                2,
                GovernanceAction::Ban { target: 1 },
                &admin,
            )),
            Err(GovernanceError::Unauthorized)
        );

        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                3,
                GovernanceAction::SetRetentionSeconds {
                    author: 3,
                    seconds: Some(3_600),
                },
                &member,
            )),
            Ok(())
        );
        assert_eq!(state.retention_seconds(3), Some(Some(3_600)));
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                3,
                GovernanceAction::SetRetentionSeconds {
                    author: 2,
                    seconds: Some(7_200),
                },
                &member,
            )),
            Err(GovernanceError::Unauthorized)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                2,
                GovernanceAction::SetRetentionSeconds {
                    author: 3,
                    seconds: None,
                },
                &admin,
            )),
            Ok(())
        );
        assert_eq!(state.retention_seconds(3), Some(None));

        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                3,
                GovernanceAction::AddDevice {
                    owner: 3,
                    device_id: "phone".into(),
                },
                &member,
            )),
            Ok(())
        );
        assert!(state.device_active(3, "phone"));
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                12,
                2,
                GovernanceAction::RemoveDevice {
                    owner: 3,
                    device_id: "phone".into(),
                },
                &admin,
            )),
            Ok(())
        );
        assert!(!state.device_active(3, "phone"));
    }

    #[test]
    fn banned_leaf_cannot_regain_role_or_devices() {
        let owner = SigningKey::generate(&mut OsRng);
        let mut state = GovernanceState::new(15, 1);
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                15,
                1,
                GovernanceAction::SetRole {
                    target: 2,
                    role: Role::Member,
                },
                &owner,
            )),
            Ok(())
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                15,
                1,
                GovernanceAction::Ban { target: 2 },
                &owner
            )),
            Ok(())
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                15,
                1,
                GovernanceAction::SetRole {
                    target: 2,
                    role: Role::Member,
                },
                &owner,
            )),
            Err(GovernanceError::Unauthorized)
        );
        assert_eq!(
            state.apply_event(GovernanceEvent::signed_by(
                15,
                1,
                GovernanceAction::AddDevice {
                    owner: 2,
                    device_id: "resurrected".into(),
                },
                &owner,
            )),
            Err(GovernanceError::Unauthorized)
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
