//! Authorized ciphertext history sync over route graph/overlay evidence.
//!
//! This module composes transport route evidence with the storage author-log
//! merge model. It is intentionally local-model only: callers must supply
//! backend/OpenMLS current-epoch membership policy and already-selected route
//! evidence before ciphertext history can be queued or applied.

use crate::{
    PeerOverlayCarrier, PeerOverlayRouteSelection, PeerOverlaySelectedRoute, TransportError,
};
use discrypt_storage::{AuthorLogEntry, AuthorLogError, AuthorLogMergeReport, LocalStore};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// History sync failure helper.
fn history_sync_error(message: impl Into<String>) -> TransportError {
    TransportError::InvalidConnectivityPolicy(message.into())
}

/// Backend/OpenMLS-derived policy for one history sync window.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySyncPolicy {
    /// Current backend/OpenMLS epoch.
    pub current_epoch: u64,
    /// Current members allowed to receive history.
    pub authorized_recipient_member_ids: BTreeSet<String>,
    /// Author leaves whose ciphertext history may be served.
    pub authorized_author_leaves: BTreeSet<u32>,
    /// Retention window in deterministic milliseconds.
    pub retention_window_ms: u64,
    /// Sender/runtime wall-clock in deterministic milliseconds.
    pub now_ms: u64,
}

impl HistorySyncPolicy {
    /// Build a history sync policy from already verified membership state.
    #[must_use]
    pub fn new(
        current_epoch: u64,
        authorized_recipient_member_ids: impl IntoIterator<Item = impl Into<String>>,
        authorized_author_leaves: impl IntoIterator<Item = u32>,
        retention_window_ms: u64,
        now_ms: u64,
    ) -> Self {
        Self {
            current_epoch,
            authorized_recipient_member_ids: authorized_recipient_member_ids
                .into_iter()
                .map(Into::into)
                .collect(),
            authorized_author_leaves: authorized_author_leaves.into_iter().collect(),
            retention_window_ms: retention_window_ms.max(1),
            now_ms,
        }
    }

    fn validate_recipient(&self, recipient_member_id: &str) -> Result<(), TransportError> {
        validate_label(recipient_member_id, "history sync recipient member id")?;
        if !self
            .authorized_recipient_member_ids
            .contains(recipient_member_id)
        {
            return Err(history_sync_error(
                "history sync recipient is not authorized in current membership",
            ));
        }
        Ok(())
    }

    fn validate_entry(&self, item: &HistorySyncItem) -> Result<(), TransportError> {
        if !self
            .authorized_author_leaves
            .contains(&item.entry.author_leaf)
        {
            return Err(history_sync_error(
                "history sync author leaf is not authorized for this group",
            ));
        }
        if item.entry.epoch == 0 || item.entry.epoch > self.current_epoch {
            return Err(history_sync_error(
                "history sync entry epoch is not covered by current membership policy",
            ));
        }
        if item.entry.ciphertext.is_empty() {
            return Err(history_sync_error(
                "history sync entry ciphertext must not be empty",
            ));
        }
        if item.created_at_ms > self.now_ms {
            return Err(history_sync_error(
                "history sync entry timestamp is in the future",
            ));
        }
        if self.now_ms.saturating_sub(item.created_at_ms) > self.retention_window_ms {
            return Err(history_sync_error(
                "history sync entry is outside retention policy",
            ));
        }
        Ok(())
    }
}

/// One ciphertext author-log entry offered for sync.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySyncItem {
    /// Append-only author-log entry.
    pub entry: AuthorLogEntry,
    /// Deterministic message creation timestamp used for retention gating.
    pub created_at_ms: u64,
}

impl HistorySyncItem {
    /// Build one history sync item.
    #[must_use]
    pub const fn new(entry: AuthorLogEntry, created_at_ms: u64) -> Self {
        Self {
            entry,
            created_at_ms,
        }
    }
}

/// Route class proven for the sync plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistorySyncRouteKind {
    /// Direct WebRTC DataChannel route evidence.
    DirectWebRtc,
    /// Configured TURN-backed WebRTC route evidence.
    ConfiguredTurnBackedWebRtc,
    /// Peer-assisted overlay route evidence.
    PeerAssistedOverlay,
}

/// Authorized ciphertext history sync plan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySyncPlan {
    /// Recipient member id proven by current membership policy.
    pub recipient_member_id: String,
    /// Route class selected by existing route graph/overlay planner evidence.
    pub route_kind: HistorySyncRouteKind,
    /// Ciphertext entries allowed by membership and retention policy.
    pub items: Vec<HistorySyncItem>,
    /// Evidence flag: providers were not used as application relays.
    pub provider_application_relay_used: bool,
    /// Evidence flag: entries remain ciphertext at this layer.
    pub ciphertext_only: bool,
    /// Honest limitation for release evidence and UI consumers.
    pub limitation: String,
}

impl HistorySyncPlan {
    fn entries(&self) -> Vec<AuthorLogEntry> {
        self.items.iter().map(|item| item.entry.clone()).collect()
    }
}

/// Bounded pending queue for offline/reconnect history sync.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySyncQueue {
    max_pending_plans: usize,
    pending: BTreeMap<String, Vec<HistorySyncPlan>>,
}

impl HistorySyncQueue {
    /// Create a queue with a normalized non-zero capacity.
    #[must_use]
    pub fn new(max_pending_plans: usize) -> Self {
        Self {
            max_pending_plans: max_pending_plans.max(1),
            pending: BTreeMap::new(),
        }
    }

    /// Queue one already validated plan for a currently offline recipient.
    pub fn enqueue(&mut self, plan: HistorySyncPlan) -> Result<(), TransportError> {
        validate_plan_shape(&plan)?;
        let pending_count = self.len();
        if pending_count >= self.max_pending_plans {
            return Err(history_sync_error("history sync queue is full"));
        }
        self.pending
            .entry(plan.recipient_member_id.clone())
            .or_default()
            .push(plan);
        Ok(())
    }

    /// Drain pending plans for a reconnecting recipient after revalidating policy.
    pub fn drain_authorized_for_recipient(
        &mut self,
        recipient_member_id: &str,
        policy: &HistorySyncPolicy,
        recipient_store: &mut LocalStore,
    ) -> Result<HistorySyncApplyReport, TransportError> {
        policy.validate_recipient(recipient_member_id)?;
        let plans = self.pending.remove(recipient_member_id).unwrap_or_default();
        let mut report = HistorySyncApplyReport::default();
        for plan in plans {
            let applied = apply_history_sync_plan(policy, &plan, recipient_store)?;
            report.plans_applied += applied.plans_applied;
            report.entries_inserted += applied.entries_inserted;
            report.entries_duplicate += applied.entries_duplicate;
        }
        Ok(report)
    }

    /// Number of pending plans.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.values().map(Vec::len).sum()
    }

    /// True when no plans are queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }
}

/// Result of applying one or more history sync plans.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistorySyncApplyReport {
    /// Plans successfully applied.
    pub plans_applied: usize,
    /// Newly inserted author-log entries.
    pub entries_inserted: usize,
    /// Idempotent duplicates already present.
    pub entries_duplicate: usize,
}

/// Build a history sync plan from current policy, route evidence, and ciphertext entries.
pub fn build_history_sync_plan(
    policy: &HistorySyncPolicy,
    recipient_member_id: impl Into<String>,
    route_selection: &PeerOverlayRouteSelection,
    items: impl IntoIterator<Item = HistorySyncItem>,
) -> Result<HistorySyncPlan, TransportError> {
    let recipient_member_id = recipient_member_id.into();
    policy.validate_recipient(&recipient_member_id)?;
    validate_route_attempts(route_selection)?;
    let route_kind = route_kind(route_selection)?;
    let items = items.into_iter().collect::<Vec<_>>();
    if items.is_empty() {
        return Err(history_sync_error(
            "history sync requires at least one ciphertext entry",
        ));
    }
    for item in &items {
        policy.validate_entry(item)?;
    }
    let plan = HistorySyncPlan {
        recipient_member_id,
        route_kind,
        items,
        provider_application_relay_used: false,
        ciphertext_only: true,
        limitation:
            "local model history-sync evidence only; not production split-machine delivery proof"
                .to_owned(),
    };
    validate_plan_shape(&plan)?;
    Ok(plan)
}

/// Apply a validated history sync plan to recipient local storage.
pub fn apply_history_sync_plan(
    policy: &HistorySyncPolicy,
    plan: &HistorySyncPlan,
    recipient_store: &mut LocalStore,
) -> Result<HistorySyncApplyReport, TransportError> {
    validate_plan_shape(plan)?;
    policy.validate_recipient(&plan.recipient_member_id)?;
    for item in &plan.items {
        policy.validate_entry(item)?;
    }
    let report = recipient_store
        .merge_author_logs_atomic(plan.entries())
        .map_err(storage_error)?;
    Ok(apply_report(report))
}

fn apply_report(report: AuthorLogMergeReport) -> HistorySyncApplyReport {
    HistorySyncApplyReport {
        plans_applied: 1,
        entries_inserted: report.inserted,
        entries_duplicate: report.duplicates,
    }
}

fn storage_error(error: AuthorLogError) -> TransportError {
    history_sync_error(format!("history sync storage merge failed: {error}"))
}

fn validate_plan_shape(plan: &HistorySyncPlan) -> Result<(), TransportError> {
    validate_label(
        &plan.recipient_member_id,
        "history sync recipient member id",
    )?;
    if plan.provider_application_relay_used {
        return Err(history_sync_error(
            "history sync must not use providers as application relays",
        ));
    }
    if !plan.ciphertext_only {
        return Err(history_sync_error(
            "history sync plan must carry ciphertext-only entries",
        ));
    }
    if plan.items.is_empty() {
        return Err(history_sync_error(
            "history sync plan requires at least one item",
        ));
    }
    Ok(())
}

fn validate_route_attempts(selection: &PeerOverlayRouteSelection) -> Result<(), TransportError> {
    for attempt in &selection.attempts {
        attempt.carrier.validate()?;
        if attempt.carrier == PeerOverlayCarrier::ProviderApplicationRelay {
            return Err(history_sync_error(
                "history sync route attempts must not include provider application relay",
            ));
        }
    }
    Ok(())
}

fn route_kind(
    selection: &PeerOverlayRouteSelection,
) -> Result<HistorySyncRouteKind, TransportError> {
    match &selection.selected {
        PeerOverlaySelectedRoute::DirectWebRtc { evidence } => {
            evidence.carrier.validate()?;
            Ok(HistorySyncRouteKind::DirectWebRtc)
        }
        PeerOverlaySelectedRoute::ConfiguredTurnBackedWebRtc { evidence } => {
            evidence.carrier.validate()?;
            Ok(HistorySyncRouteKind::ConfiguredTurnBackedWebRtc)
        }
        PeerOverlaySelectedRoute::PeerAssistedOverlay { .. } => {
            Ok(HistorySyncRouteKind::PeerAssistedOverlay)
        }
    }
}

fn validate_label(value: &str, label: &str) -> Result<(), TransportError> {
    if value.trim().is_empty() || value.trim() != value || value.len() > 128 {
        Err(history_sync_error(format!(
            "{label} must be non-empty trimmed text up to 128 bytes"
        )))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        rank_relay_candidates, PeerOverlayAdmittedSet, PeerOverlayAuth,
        PeerOverlayConfiguredTurnOrder, PeerOverlayPeerRef, PeerOverlayRelayAuthoritySet,
        PeerOverlayRelayCandidate, PeerOverlayRelayCandidateDiagnostics,
        PeerOverlayRelayRouteEvidence, PeerOverlayRouteLegEvidence,
        PeerOverlayRouteSelectionAttempt, PeerOverlayRouteSelectionInput,
        PeerOverlayRouteSelectionPolicy, SignalingPeerId,
    };

    fn peer(index: u8, epoch: u64) -> Result<PeerOverlayPeerRef, TransportError> {
        Ok(PeerOverlayPeerRef::new(
            SignalingPeerId::new(format!("peer-{index}"))?,
            format!("member-{index}"),
            format!("device-{index}"),
            epoch,
        ))
    }

    fn nonzero_32(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn auth(epoch: u64) -> PeerOverlayAuth {
        PeerOverlayAuth {
            group_id_commitment: nonzero_32(1),
            epoch,
            sender_leaf_index: 1,
            confirmation_tag_commitment: nonzero_32(2),
            frame_auth_tag: vec![3; 16],
        }
    }

    fn admitted(epoch: u64) -> Result<PeerOverlayAdmittedSet, TransportError> {
        PeerOverlayAdmittedSet::new(
            epoch,
            [peer(1, epoch)?, peer(2, epoch)?, peer(3, epoch)?],
            [],
        )
    }

    fn route_leg(
        from: u8,
        to: u8,
        epoch: u64,
        carrier: PeerOverlayCarrier,
    ) -> Result<PeerOverlayRouteLegEvidence, TransportError> {
        Ok(PeerOverlayRouteLegEvidence {
            from: peer(from, epoch)?,
            to: peer(to, epoch)?,
            carrier,
            route_label: format!("history-route-{from}-{to}"),
            live: true,
        })
    }

    fn direct_selection(epoch: u64) -> Result<PeerOverlayRouteSelection, TransportError> {
        let admitted = admitted(epoch)?;
        crate::select_peer_overlay_route(
            &admitted,
            &auth(epoch),
            &PeerOverlayRouteSelectionPolicy::default(),
            PeerOverlayRouteSelectionInput {
                source: peer(1, epoch)?,
                destination: peer(3, epoch)?,
                direct_pair: Some(route_leg(
                    1,
                    3,
                    epoch,
                    PeerOverlayCarrier::DirectWebRtcDataChannel,
                )?),
                configured_turn_pair: None,
                ranked_relays: Vec::new(),
                relay_routes: Vec::new(),
            },
        )
    }

    fn peer_overlay_selection(epoch: u64) -> Result<PeerOverlayRouteSelection, TransportError> {
        let admitted = admitted(epoch)?;
        let authority = PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &admitted,
            nonzero_32(1),
            nonzero_32(2),
            [peer(2, epoch)?],
        )?;
        let ranked = rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &crate::PeerOverlayRelayCandidatePolicy::default(),
            [PeerOverlayRelayCandidate {
                relay: peer(2, epoch)?,
                diagnostics: PeerOverlayRelayCandidateDiagnostics {
                    latency_ms: 12,
                    successful_health_probes: 10,
                    failed_health_probes: 0,
                    egress_capacity_bytes_per_second: 96_000,
                    current_load_bytes_per_second: 8_000,
                    energy_cost_bps: 100,
                    freeload_penalty_bps: 0,
                },
            }],
        )?;
        crate::select_peer_overlay_route(
            &admitted,
            &auth(epoch),
            &PeerOverlayRouteSelectionPolicy {
                configured_turn_order: PeerOverlayConfiguredTurnOrder::AfterPeerRelay,
            },
            PeerOverlayRouteSelectionInput {
                source: peer(1, epoch)?,
                destination: peer(3, epoch)?,
                direct_pair: None,
                configured_turn_pair: None,
                ranked_relays: ranked,
                relay_routes: vec![PeerOverlayRelayRouteEvidence {
                    relay: peer(2, epoch)?,
                    source_to_relay: route_leg(
                        1,
                        2,
                        epoch,
                        PeerOverlayCarrier::DirectWebRtcDataChannel,
                    )?,
                    relay_to_destination: route_leg(
                        2,
                        3,
                        epoch,
                        PeerOverlayCarrier::ConfiguredTurnBackedWebRtc,
                    )?,
                }],
            },
        )
    }

    fn policy(epoch: u64) -> HistorySyncPolicy {
        HistorySyncPolicy::new(epoch, ["member-3"], [1], 10_000, 20_000)
    }

    fn item(sequence: u64, epoch: u64, created_at_ms: u64) -> HistorySyncItem {
        HistorySyncItem::new(
            AuthorLogEntry::new_stable(
                1,
                "device-1",
                sequence,
                epoch,
                format!("ciphertext-history-{sequence}").into_bytes(),
            ),
            created_at_ms,
        )
    }

    #[test]
    fn returning_member_receives_queued_ciphertext_history_on_reconnect(
    ) -> Result<(), TransportError> {
        let policy = policy(9);
        let plan = build_history_sync_plan(
            &policy,
            "member-3",
            &direct_selection(9)?,
            [item(1, 8, 15_000), item(2, 9, 19_000)],
        )?;
        assert_eq!(plan.route_kind, HistorySyncRouteKind::DirectWebRtc);
        assert!(plan.ciphertext_only);
        assert!(!plan.provider_application_relay_used);

        let mut queue = HistorySyncQueue::new(4);
        queue.enqueue(plan)?;
        let mut returning_store = LocalStore::default();
        let report =
            queue.drain_authorized_for_recipient("member-3", &policy, &mut returning_store)?;

        assert_eq!(
            report,
            HistorySyncApplyReport {
                plans_applied: 1,
                entries_inserted: 2,
                entries_duplicate: 0,
            }
        );
        assert!(queue.is_empty());
        assert_eq!(returning_store.author_log_for(1).len(), 2);
        assert!(returning_store
            .author_log_snapshot()
            .iter()
            .all(|entry| entry.ciphertext.starts_with(b"ciphertext-history-")));
        Ok(())
    }

    #[test]
    fn peer_overlay_route_evidence_can_sync_authorized_history() -> Result<(), TransportError> {
        let policy = policy(9);
        let plan = build_history_sync_plan(
            &policy,
            "member-3",
            &peer_overlay_selection(9)?,
            [item(1, 9, 19_000)],
        )?;
        let mut store = LocalStore::default();
        let report = apply_history_sync_plan(&policy, &plan, &mut store)?;

        assert_eq!(plan.route_kind, HistorySyncRouteKind::PeerAssistedOverlay);
        assert_eq!(report.entries_inserted, 1);
        assert!(!plan.provider_application_relay_used);
        Ok(())
    }

    #[test]
    fn provider_application_relay_attempt_fails_closed() -> Result<(), TransportError> {
        let mut selection = direct_selection(9)?;
        selection.attempts.push(PeerOverlayRouteSelectionAttempt {
            carrier: PeerOverlayCarrier::ProviderApplicationRelay,
            selected: false,
        });

        assert!(
            build_history_sync_plan(&policy(9), "member-3", &selection, [item(1, 9, 19_000)])
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn unauthorized_or_out_of_policy_history_is_rejected_without_mutating_store(
    ) -> Result<(), TransportError> {
        let selection = direct_selection(9)?;
        assert!(
            build_history_sync_plan(&policy(9), "member-4", &selection, [item(1, 9, 19_000)])
                .is_err()
        );
        assert!(build_history_sync_plan(
            &policy(9),
            "member-3",
            &selection,
            [HistorySyncItem::new(
                AuthorLogEntry::new_stable(2, "device-2", 1, 9, b"ciphertext".to_vec()),
                19_000,
            )]
        )
        .is_err());
        assert!(
            build_history_sync_plan(&policy(9), "member-3", &selection, [item(1, 10, 19_000)])
                .is_err()
        );
        assert!(
            build_history_sync_plan(&policy(9), "member-3", &selection, [item(1, 9, 1)]).is_err()
        );

        let accepted =
            build_history_sync_plan(&policy(9), "member-3", &selection, [item(1, 9, 19_000)])?;
        let mut removed_member_policy = policy(9);
        removed_member_policy
            .authorized_recipient_member_ids
            .remove("member-3");
        let mut store = LocalStore::default();
        assert!(apply_history_sync_plan(&removed_member_policy, &accepted, &mut store).is_err());
        assert!(store.author_log_snapshot().is_empty());
        Ok(())
    }

    #[test]
    fn history_sync_merge_is_idempotent_and_rejects_forks() -> Result<(), TransportError> {
        let policy = policy(9);
        let selection = direct_selection(9)?;
        let plan = build_history_sync_plan(&policy, "member-3", &selection, [item(1, 9, 19_000)])?;
        let mut store = LocalStore::default();

        assert_eq!(
            apply_history_sync_plan(&policy, &plan, &mut store)?.entries_inserted,
            1
        );
        assert_eq!(
            apply_history_sync_plan(&policy, &plan, &mut store)?.entries_duplicate,
            1
        );

        let fork = HistorySyncItem::new(
            AuthorLogEntry::new_stable(1, "device-1", 1, 9, b"different-ciphertext".to_vec()),
            19_000,
        );
        let fork_plan = build_history_sync_plan(&policy, "member-3", &selection, [fork])?;
        assert!(apply_history_sync_plan(&policy, &fork_plan, &mut store).is_err());
        assert_eq!(store.author_log_snapshot().len(), 1);
        Ok(())
    }

    #[test]
    fn history_sync_failed_batch_does_not_partially_mutate_store() -> Result<(), TransportError> {
        let policy = policy(9);
        let selection = direct_selection(9)?;
        let mut store = LocalStore::default();
        let original =
            AuthorLogEntry::new_stable(1, "device-1", 2, 9, b"original-ciphertext".to_vec());
        store.append_sent(original.clone()).map_err(storage_error)?;

        let earlier = HistorySyncItem::new(
            AuthorLogEntry::new_stable(1, "device-1", 1, 9, b"new-ciphertext".to_vec()),
            19_000,
        );
        let fork = HistorySyncItem::new(
            AuthorLogEntry::new_stable(1, "device-1", 2, 9, b"different-ciphertext".to_vec()),
            19_000,
        );
        let plan = build_history_sync_plan(&policy, "member-3", &selection, [earlier, fork])?;

        assert!(apply_history_sync_plan(&policy, &plan, &mut store).is_err());
        assert_eq!(store.author_log_snapshot(), vec![original]);
        Ok(())
    }
}
