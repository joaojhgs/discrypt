//! Delivery, ordering, fork detection, Welcome/catch-up, and repair facades around MLS state.
//!
//! This crate deliberately models the service layer around MLS rather than a
//! replacement for MLS cryptography: it orders application events, detects
//! divergent epoch summaries, rejects replay/downgrade/forked commits, and
//! produces explicit rejoin/reproposal repair plans.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Delivery errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DeliveryError {
    /// A same-epoch summary has a different tree hash or confirmation tag.
    #[error("divergent tree hash at epoch {0}")]
    DivergentTree(u64),
    /// A commit attempts to move the client backwards.
    #[error("downgrade or replay at epoch {0}")]
    DowngradeOrReplay(u64),
    /// A Welcome package is expired.
    #[error("welcome expired")]
    WelcomeExpired,
    /// A repair plan attempted to replay invalid divergent MLS commits.
    #[error("repair attempted to replay divergent MLS commits")]
    DivergentCommitReplay,
    /// A repair plan would move to an older epoch.
    #[error("repair target epoch {target} is older than local epoch {local}")]
    StaleRepairTarget { local: u64, target: u64 },
}

/// Compact state summary exchanged during catch-up.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EpochSummary {
    /// MLS epoch number.
    pub epoch: u64,
    /// MLS tree hash for the epoch.
    pub tree_hash: [u8; 32],
    /// MLS confirmation tag for the epoch.
    pub confirmation_tag: [u8; 32],
}

/// Compare two summaries and return whether repair is required.
#[must_use]
pub fn needs_repair(local: &EpochSummary, remote: &EpochSummary) -> bool {
    local.epoch == remote.epoch
        && (local.tree_hash != remote.tree_hash
            || local.confirmation_tag != remote.confirmation_tag)
}

/// Status of a remote summary relative to a local state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ForkStatus {
    /// Same epoch and same hashes.
    InSync,
    /// Remote is ahead; request ordered catch-up.
    NeedsCatchUp { remote_epoch: u64 },
    /// Remote is behind and must not be accepted as current history.
    DowngradeOrReplay { remote_epoch: u64 },
    /// Same epoch but different cryptographic state.
    Diverged(ForkEvidence),
}

/// Evidence captured when a fork is detected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForkEvidence {
    /// Local accepted summary.
    pub local: EpochSummary,
    /// Remote conflicting summary.
    pub remote: EpochSummary,
}

/// Detect whether a remote summary is a catch-up source, replay/downgrade, or fork.
#[must_use]
pub fn detect_fork_or_replay(local: &EpochSummary, remote: &EpochSummary) -> ForkStatus {
    match remote.epoch.cmp(&local.epoch) {
        Ordering::Greater => ForkStatus::NeedsCatchUp {
            remote_epoch: remote.epoch,
        },
        Ordering::Less => ForkStatus::DowngradeOrReplay {
            remote_epoch: remote.epoch,
        },
        Ordering::Equal if needs_repair(local, remote) => ForkStatus::Diverged(ForkEvidence {
            local: local.clone(),
            remote: remote.clone(),
        }),
        Ordering::Equal => ForkStatus::InSync,
    }
}

/// Application event carried alongside ordered MLS delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationEvent {
    /// Epoch under which the event was authored.
    pub epoch: u64,
    /// Author or committer leaf index in the last common accepted tree.
    pub leaf_index: u32,
    /// Stable event id.
    pub event_id: String,
    /// Opaque application payload bytes.
    pub payload: Vec<u8>,
}

impl ApplicationEvent {
    /// Create an event for deterministic tests and facades.
    #[must_use]
    pub fn new(epoch: u64, leaf_index: u32, event_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            epoch,
            leaf_index,
            event_id: event_id.into(),
            payload,
        }
    }

    /// Content hash used by the canonical comparator.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.event_id.as_bytes());
        h.update(&self.payload);
        h.finalize().into()
    }
}

/// Canonical key: epoch → committer/author leaf index → signed content hash.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct CanonicalEventKey {
    /// Event epoch.
    pub epoch: u64,
    /// Author/committer leaf index.
    pub leaf_index: u32,
    /// Event content hash.
    pub content_hash: [u8; 32],
}

impl From<&ApplicationEvent> for CanonicalEventKey {
    fn from(event: &ApplicationEvent) -> Self {
        Self {
            epoch: event.epoch,
            leaf_index: event.leaf_index,
            content_hash: event.content_hash(),
        }
    }
}

/// Deterministically order application events by the plan's canonical comparator.
#[must_use]
pub fn order_application_events(mut events: Vec<ApplicationEvent>) -> Vec<ApplicationEvent> {
    events.sort_by(|a, b| {
        CanonicalEventKey::from(a)
            .cmp(&CanonicalEventKey::from(b))
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    events
}

/// Commit envelope accepted by the delivery layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitEnvelope {
    /// Summary after applying the commit.
    pub summary: EpochSummary,
    /// Committer leaf index.
    pub committer_leaf: u32,
    /// Valid application events that may be applied after MLS state is accepted.
    pub application_events: Vec<ApplicationEvent>,
}

impl CommitEnvelope {
    /// Build a commit envelope and order application events canonically.
    #[must_use]
    pub fn new(
        summary: EpochSummary,
        committer_leaf: u32,
        application_events: Vec<ApplicationEvent>,
    ) -> Self {
        Self {
            summary,
            committer_leaf,
            application_events: order_application_events(application_events),
        }
    }
}

/// Deterministic delivery state for tests and higher-level facades.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeliveryState {
    summary: EpochSummary,
    accepted_events: BTreeMap<CanonicalEventKey, ApplicationEvent>,
}

impl DeliveryState {
    /// Create a state from the last accepted MLS summary.
    #[must_use]
    pub fn new(summary: EpochSummary) -> Self {
        Self {
            summary,
            accepted_events: BTreeMap::new(),
        }
    }

    /// Current accepted summary.
    #[must_use]
    pub fn summary(&self) -> &EpochSummary {
        &self.summary
    }

    /// Accepted application events in canonical order.
    #[must_use]
    pub fn accepted_events(&self) -> Vec<ApplicationEvent> {
        self.accepted_events.values().cloned().collect()
    }

    /// Apply a commit only if it extends this state without replay/downgrade/fork.
    pub fn apply_commit(&mut self, commit: CommitEnvelope) -> Result<(), DeliveryError> {
        match detect_fork_or_replay(&self.summary, &commit.summary) {
            ForkStatus::NeedsCatchUp { .. }
                if commit.summary.epoch == self.summary.epoch.saturating_add(1) =>
            {
                if let Some(event) = commit
                    .application_events
                    .iter()
                    .find(|event| event.epoch != commit.summary.epoch)
                {
                    return Err(DeliveryError::DowngradeOrReplay(event.epoch));
                }
                self.summary = commit.summary;
                for event in commit.application_events {
                    self.accepted_events
                        .insert(CanonicalEventKey::from(&event), event);
                }
                Ok(())
            }
            ForkStatus::InSync => Err(DeliveryError::DowngradeOrReplay(commit.summary.epoch)),
            ForkStatus::NeedsCatchUp { remote_epoch } => {
                Err(DeliveryError::DowngradeOrReplay(remote_epoch))
            }
            ForkStatus::DowngradeOrReplay { remote_epoch } => {
                Err(DeliveryError::DowngradeOrReplay(remote_epoch))
            }
            ForkStatus::Diverged(evidence) => {
                Err(DeliveryError::DivergentTree(evidence.local.epoch))
            }
        }
    }

    /// Apply an explicit fork repair plan.
    ///
    /// This models the service contract around OpenMLS repair: losing members
    /// rejoin/reboot to the winning cryptographic state and only application
    /// events re-proposed under that repaired epoch are accepted. Divergent MLS
    /// commits from the losing branch are never replayed.
    pub fn apply_repair_plan(&mut self, plan: RepairPlan) -> Result<(), DeliveryError> {
        if plan.replays_divergent_mls_commits {
            return Err(DeliveryError::DivergentCommitReplay);
        }
        if plan.winner.epoch < self.summary.epoch {
            return Err(DeliveryError::StaleRepairTarget {
                local: self.summary.epoch,
                target: plan.winner.epoch,
            });
        }
        if matches!(plan.action, RepairAction::None) {
            return Ok(());
        }

        for event in &plan.reproposed_events {
            if event.epoch != plan.winner.epoch {
                return Err(DeliveryError::DowngradeOrReplay(event.epoch));
            }
        }

        self.summary = plan.winner;
        for event in plan.reproposed_events {
            self.accepted_events
                .insert(CanonicalEventKey::from(&event), event);
        }
        Ok(())
    }
}

/// Expiring Welcome package for final admission into a current MLS state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WelcomePackage {
    /// Room/group id.
    pub room_id: String,
    /// Newly admitted leaf.
    pub new_leaf: u32,
    /// Accepted epoch summary the new member should join.
    pub summary: EpochSummary,
    /// Expiration timestamp in deterministic milliseconds.
    pub expires_at_ms: u64,
}

impl WelcomePackage {
    /// Build a Welcome package.
    #[must_use]
    pub fn new(
        room_id: impl Into<String>,
        new_leaf: u32,
        summary: EpochSummary,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            room_id: room_id.into(),
            new_leaf,
            summary,
            expires_at_ms,
        }
    }

    /// Validate the Welcome at a deterministic timestamp.
    pub fn validate(&self, now_ms: u64) -> Result<(), DeliveryError> {
        if now_ms <= self.expires_at_ms {
            Ok(())
        } else {
            Err(DeliveryError::WelcomeExpired)
        }
    }
}

/// Catch-up bundle delivered after a Welcome or when a remote member is ahead.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatchUpBundle {
    /// Latest accepted summary.
    pub summary: EpochSummary,
    /// Ordered commits needed by the receiver.
    pub commits: Vec<CommitEnvelope>,
    /// Ordered application events safe to replay under the accepted MLS state.
    pub application_events: Vec<ApplicationEvent>,
}

impl CatchUpBundle {
    /// Build a bundle with ordered application events.
    #[must_use]
    pub fn new(
        summary: EpochSummary,
        commits: Vec<CommitEnvelope>,
        application_events: Vec<ApplicationEvent>,
    ) -> Self {
        Self {
            summary,
            commits,
            application_events: order_application_events(application_events),
        }
    }
}

/// Repair action: rejoin first, then re-propose valid app events only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RepairAction {
    /// No repair required.
    None,
    /// Rejoin/reboot MLS state, then re-propose valid application events.
    RejoinAndReproposal { coordinator_leaf: u32 },
}

/// Repair plan for a detected fork.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairPlan {
    /// Action to execute.
    pub action: RepairAction,
    /// Winning MLS summary to rejoin.
    pub winner: EpochSummary,
    /// Application-level events eligible for re-proposal after rejoin.
    pub reproposed_events: Vec<ApplicationEvent>,
    /// Explicit marker that divergent MLS commits are not replayed.
    pub replays_divergent_mls_commits: bool,
}

/// Select a deterministic repair coordinator from last common accepted leaf indexes.
#[must_use]
pub fn select_repair_action(diverged: bool, leaves: &[u32]) -> RepairAction {
    if !diverged {
        return RepairAction::None;
    }
    RepairAction::RejoinAndReproposal {
        coordinator_leaf: leaves.iter().copied().max().unwrap_or_default(),
    }
}

/// Build an explicit repair plan for a fork.
#[must_use]
pub fn plan_repair(
    evidence: ForkEvidence,
    last_common_leaves: &[u32],
    still_valid_events: Vec<ApplicationEvent>,
) -> RepairPlan {
    let winner = if evidence.remote > evidence.local {
        evidence.remote
    } else {
        evidence.local
    };
    let winner_epoch = winner.epoch;
    RepairPlan {
        action: select_repair_action(true, last_common_leaves),
        winner,
        reproposed_events: order_application_events(
            still_valid_events
                .into_iter()
                .filter(|event| event.epoch == winner_epoch)
                .collect(),
        ),
        replays_divergent_mls_commits: false,
    }
}

/// Apply a repair plan to all honest members and return their repaired summaries.
#[must_use]
pub fn repair_to_winner(participants: usize, plan: &RepairPlan) -> Vec<EpochSummary> {
    (0..participants).map(|_| plan.winner.clone()).collect()
}

/// Build deterministic test summaries.
#[must_use]
pub fn summary(epoch: u64, tree_byte: u8, confirmation_byte: u8) -> EpochSummary {
    EpochSummary {
        epoch,
        tree_hash: [tree_byte; 32],
        confirmation_tag: [confirmation_byte; 32],
    }
}

/// Assert all summaries converge to the same confirmation tag.
#[must_use]
pub fn equal_confirmation_tags(summaries: &[EpochSummary]) -> bool {
    let Some(first) = summaries.first() else {
        return true;
    };
    summaries
        .iter()
        .all(|summary| summary.confirmation_tag == first.confirmation_tag)
}

/// Dedupe event ids after repair/reproposal.
#[must_use]
pub fn event_ids(events: &[ApplicationEvent]) -> BTreeSet<String> {
    events.iter().map(|event| event.event_id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_events_by_epoch_leaf_and_content_hash() {
        let ordered = order_application_events(vec![
            ApplicationEvent::new(2, 9, "late", b"z".to_vec()),
            ApplicationEvent::new(1, 8, "b", b"b".to_vec()),
            ApplicationEvent::new(1, 2, "a", b"a".to_vec()),
        ]);
        assert_eq!(ordered[0].event_id, "a");
        assert_eq!(ordered[1].event_id, "b");
        assert_eq!(ordered[2].event_id, "late");
    }

    #[test]
    fn detects_same_epoch_divergence_and_plans_explicit_repair() {
        let a = summary(2, 1, 2);
        let b = summary(2, 9, 2);
        let status = detect_fork_or_replay(&a, &b);
        assert!(matches!(status, ForkStatus::Diverged(_)));
        let evidence = match status {
            ForkStatus::Diverged(evidence) => evidence,
            _ => ForkEvidence {
                local: a.clone(),
                remote: b.clone(),
            },
        };
        let plan = plan_repair(
            evidence,
            &[1, 7, 3],
            vec![ApplicationEvent::new(2, 3, "msg", b"ciphertext".to_vec())],
        );
        assert_eq!(
            plan.action,
            RepairAction::RejoinAndReproposal {
                coordinator_leaf: 7
            }
        );
        assert_eq!(plan.winner, b);
        assert_eq!(
            plan.reproposed_events,
            order_application_events(vec![ApplicationEvent::new(
                2,
                3,
                "msg",
                b"ciphertext".to_vec()
            )])
        );
        assert!(!plan.replays_divergent_mls_commits);
        let repaired = repair_to_winner(4, &plan);
        assert_eq!(repaired, vec![plan.winner.clone(); 4]);
        assert!(equal_confirmation_tags(&repaired));
    }

    #[test]
    fn rejects_replay_downgrade_and_forked_commit() {
        let mut state = DeliveryState::new(summary(1, 1, 1));
        let commit = CommitEnvelope::new(
            summary(2, 2, 2),
            1,
            vec![ApplicationEvent::new(2, 1, "m1", b"ciphertext".to_vec())],
        );
        assert_eq!(state.apply_commit(commit), Ok(()));
        assert_eq!(state.accepted_events().len(), 1);
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(1, 1, 1), 1, Vec::new())),
            Err(DeliveryError::DowngradeOrReplay(1))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(2, 2, 2), 1, Vec::new())),
            Err(DeliveryError::DowngradeOrReplay(2))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(2, 9, 2), 1, Vec::new())),
            Err(DeliveryError::DivergentTree(2))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(
                summary(3, 3, 3),
                1,
                vec![ApplicationEvent::new(2, 1, "old", b"ciphertext".to_vec())],
            )),
            Err(DeliveryError::DowngradeOrReplay(2))
        );
        assert_eq!(state.summary(), &summary(2, 2, 2));
    }

    #[test]
    fn repair_rejoins_winner_and_reproposes_only_current_epoch_events() {
        let local = summary(3, 3, 3);
        let remote = summary(3, 9, 9);
        let evidence = ForkEvidence {
            local: local.clone(),
            remote: remote.clone(),
        };
        let plan = plan_repair(
            evidence,
            &[1, 4],
            vec![
                ApplicationEvent::new(2, 1, "stale", b"old".to_vec()),
                ApplicationEvent::new(3, 4, "valid", b"current".to_vec()),
            ],
        );
        assert_eq!(
            plan.action,
            RepairAction::RejoinAndReproposal {
                coordinator_leaf: 4
            }
        );
        assert_eq!(plan.winner, remote);
        assert_eq!(
            event_ids(&plan.reproposed_events),
            BTreeSet::from(["valid".into()])
        );

        let mut state = DeliveryState::new(local);
        assert_eq!(state.apply_repair_plan(plan), Ok(()));
        assert_eq!(state.summary(), &remote);
        assert_eq!(
            event_ids(&state.accepted_events()),
            BTreeSet::from(["valid".into()])
        );
    }

    #[test]
    fn repair_rejects_divergent_commit_replay_and_stale_targets() {
        let local = summary(4, 4, 4);
        let mut replay_plan = plan_repair(
            ForkEvidence {
                local: local.clone(),
                remote: summary(4, 9, 9),
            },
            &[1],
            Vec::new(),
        );
        replay_plan.replays_divergent_mls_commits = true;
        let mut state = DeliveryState::new(local.clone());
        assert_eq!(
            state.apply_repair_plan(replay_plan),
            Err(DeliveryError::DivergentCommitReplay)
        );

        let stale_plan = RepairPlan {
            action: RepairAction::RejoinAndReproposal {
                coordinator_leaf: 1,
            },
            winner: summary(3, 3, 3),
            reproposed_events: Vec::new(),
            replays_divergent_mls_commits: false,
        };
        assert_eq!(
            state.apply_repair_plan(stale_plan),
            Err(DeliveryError::StaleRepairTarget {
                local: 4,
                target: 3,
            })
        );
        assert_eq!(state.summary(), &local);
    }

    #[test]
    fn welcome_expires_and_catchup_orders_events() {
        let welcome = WelcomePackage::new("room", 4, summary(3, 3, 3), 1_000);
        assert_eq!(welcome.validate(999), Ok(()));
        assert_eq!(welcome.validate(1_001), Err(DeliveryError::WelcomeExpired));
        let catchup = CatchUpBundle::new(
            summary(3, 3, 3),
            Vec::new(),
            vec![
                ApplicationEvent::new(3, 9, "b", b"b".to_vec()),
                ApplicationEvent::new(3, 1, "a", b"a".to_vec()),
            ],
        );
        assert_eq!(catchup.application_events[0].event_id, "a");
    }
}
