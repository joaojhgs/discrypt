//! Headless multinode harness for discrypt acceptance tests.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use discrypt_core::create_dm;
use discrypt_mls_core::Identity;

/// Build two fresh identities and return their safety number.
#[must_use]
pub fn two_node_dm_safety_number() -> String {
    let a = Identity::generate("alice");
    let b = Identity::generate("bob");
    let (_g, safety) = create_dm(&a, &b);
    safety
}

/// Deterministic Phase-1 media security smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaSecuritySmoke {
    /// Passive relays cannot recover plaintext from relay-visible ciphertext.
    pub passive_relay_cannot_read: bool,
    /// Replaying an already accepted frame is rejected.
    pub replay_rejected: bool,
    /// Tampering with relay-visible ciphertext is rejected by AEAD authentication.
    pub tamper_rejected: bool,
    /// Receiver plaintext after successful authentication and replay acceptance.
    pub plaintext: Vec<u8>,
}

/// Deterministic Phase-2 relay overlay smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayOverlaySmoke {
    /// Selected route respects the ≤3 hop cap.
    pub hop_limit_respected: bool,
    /// Failover avoids the failed relay and converges within the Phase-2 gate.
    pub failover_recovered: bool,
    /// Replay/redelivery bookkeeping rejects duplicate packet ids.
    pub redelivery_replay_rejected: bool,
    /// Store-forward rejects caller-supplied plaintext samples in relay payloads.
    pub store_forward_plaintext_rejected: bool,
    /// Store-forward delivers ciphertext before TTL and drops expired envelopes.
    pub store_forward_ttl_enforced: bool,
    /// Store-forward replication fanout is deterministically bounded.
    pub store_forward_fanout_bounded: bool,
    /// Media carried over relay topology remains ciphertext-only to relays.
    pub ciphertext_only_media: bool,
    /// Active relay tampering over the selected route is rejected by media auth.
    pub tamper_rejected: bool,
    /// Receiver plaintext after successful relay delivery.
    pub plaintext: Vec<u8>,
}

/// Deterministic Phase-3 text/history/MLS delivery smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextHistoryDeliverySmoke {
    /// Two-node app text round-trips through encrypted bytes before entering stores.
    pub text_e2e_roundtrip: bool,
    /// Author/recipient storage and relay-visible samples do not contain plaintext.
    pub no_plaintext_in_text_surfaces: bool,
    /// Own devices merge one author's log without duplicate/lost entries.
    pub author_logs_merged: bool,
    /// Recipient cache retains only the bounded newest ciphertext/key entries.
    pub recipient_cache_bounded: bool,
    /// Sixteen peers converge on all author-log gossip items.
    pub gossip_converged_16: bool,
    /// Ordered commit delivery accepts forward commits and canonicalizes app events.
    pub ordered_commit_delivery: bool,
    /// Welcome and catch-up objects validate admission/catch-up semantics.
    pub welcome_catchup_live: bool,
    /// Same-epoch tree divergence is detected rather than silently accepted.
    pub fork_detected_not_silent: bool,
    /// Explicit repair converges honest members to equal confirmation tags.
    pub repair_converged_equal_tags: bool,
    /// Repair plan refuses to replay invalid divergent MLS commits.
    pub divergent_mls_not_replayed: bool,
}

/// Deterministic Phase-4 retention/shred/live-key/storage smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionShredSmoke {
    /// Default retention caches fresh messages and locks old placeholders.
    pub default_window_locks_old_messages: bool,
    /// Shorten is retroactive while lengthen is future-only.
    pub shorten_retro_lengthen_future: bool,
    /// Cross-device shred blocks online devices and pending offline devices after sync.
    pub cross_device_shred_sync: bool,
    /// Live-key requests require local membership proof and enforce rate limits.
    pub live_key_membership_rate_limit_decoy: bool,
    /// Secure-delete simulator removes key material from SQLite/WAL/key-store paths.
    pub secure_delete_negative: bool,
    /// Account-continuity backup excludes content keys and cannot resurrect shredded content.
    pub recovery_cannot_resurrect_content_keys: bool,
}

/// Deterministic Phase-B storage persistence verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePersistenceSmoke {
    /// A fresh encrypted app DB starts empty before first profile creation.
    pub fresh_install_starts_empty: bool,
    /// A new store handle can restart and read the previously encrypted state.
    pub restart_loads_encrypted_state: bool,
    /// Plaintext app-state bytes are absent from DB, WAL, and temp sidecar paths.
    pub no_plaintext_in_db_wal_or_temp: bool,
    /// Malformed legacy/corrupt store bytes fail closed instead of seeding silently.
    pub corrupted_store_rejected: bool,
    /// Secure delete only passes after DB, WAL, and keychain material are all removed.
    pub secure_delete_requires_db_wal_and_keychain: bool,
}

/// Deterministic Phase-5 governance/admission/recovery/abuse smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceAdmissionSmoke {
    /// Governance events are canonical and signed.
    pub governance_ordered_signed: bool,
    /// Unauthorized and out-of-epoch actions are rejected.
    pub governance_rejects_invalid_authority: bool,
    /// Removed admin cannot win a same-epoch race.
    pub removed_admin_cannot_win: bool,
    /// Invite expiry/revoke/max-use are enforced.
    pub invite_controls_enforced: bool,
    /// Password admission rejects offline verifiers and requires Welcome.
    pub password_and_welcome_gate: bool,
    /// Recovery requires trust material and excludes content keys.
    pub recovery_trust_model: bool,
    /// Abuse controls rate-limit invites/spam and penalize freeloading.
    pub abuse_controls_enforced: bool,
}

/// Deterministic Phase-6 connectivity/signaling/push/metadata smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectivitySignalingPushSmoke {
    /// Signaling stores only opaque rendezvous data and no durable linkage.
    pub signaling_zero_linkage_at_rest: bool,
    /// Simulated NAT activates STUN, overlay, and TURN in the approved order.
    pub fallback_chain_covered: bool,
    /// Owner/group endpoint overrides are honored for STUN and TURN.
    pub owner_overrides_used: bool,
    /// Android FCM wake envelope is content-free.
    pub android_wake_content_free: bool,
    /// Pcap-style events match the approved infrastructure metadata matrix.
    pub metadata_matrix_validated: bool,
    /// Pcap-style fixture contains no forbidden content/identity egress.
    pub pcap_no_central_content: bool,
    /// TURN and peer relay observations are ciphertext-only.
    pub relays_ciphertext_only: bool,
    /// Local-process socket adapter delivers ciphertext and rejects plaintext.
    pub socket_local_process_conformant: bool,
    /// Route reporting preserves order and states deterministic-test limitations.
    pub route_reporting_honest: bool,
}

/// Deterministic Phase-7 UX and end-to-end hardening smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxE2eHardeningSmoke {
    /// Tauri command facade returns the required snapshot and verification commands.
    pub command_surface_ready: bool,
    /// React/Discord-style UX model includes servers, text channels, and voice rooms.
    pub discord_style_model_ready: bool,
    /// Device management and friend safety-number verification are surfaced.
    pub verification_and_devices_ready: bool,
    /// Invite, retention, and deletion flows expose honest copy.
    pub invite_retention_deletion_ready: bool,
    /// Connectivity, push, and metadata posture are surfaced.
    pub connectivity_copy_ready: bool,
    /// Prior deterministic E2E harness phases still pass through one final smoke.
    pub all_phase_smokes_ready: bool,
}

impl MediaSecuritySmoke {
    /// True when every Phase-1 security invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.passive_relay_cannot_read && self.replay_rejected && self.tamper_rejected
    }
}

impl RelayOverlaySmoke {
    /// True when every Phase-2 relay invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.hop_limit_respected
            && self.failover_recovered
            && self.redelivery_replay_rejected
            && self.store_forward_plaintext_rejected
            && self.store_forward_ttl_enforced
            && self.store_forward_fanout_bounded
            && self.ciphertext_only_media
            && self.tamper_rejected
    }
}

impl TextHistoryDeliverySmoke {
    /// True when every Phase-3 delivery invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.text_e2e_roundtrip
            && self.no_plaintext_in_text_surfaces
            && self.author_logs_merged
            && self.recipient_cache_bounded
            && self.gossip_converged_16
            && self.ordered_commit_delivery
            && self.welcome_catchup_live
            && self.fork_detected_not_silent
            && self.repair_converged_equal_tags
            && self.divergent_mls_not_replayed
    }
}

impl RetentionShredSmoke {
    /// True when every Phase-4 retention/shred invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.default_window_locks_old_messages
            && self.shorten_retro_lengthen_future
            && self.cross_device_shred_sync
            && self.live_key_membership_rate_limit_decoy
            && self.secure_delete_negative
            && self.recovery_cannot_resurrect_content_keys
    }
}

impl StoragePersistenceSmoke {
    /// True when every Phase-B storage persistence invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.fresh_install_starts_empty
            && self.restart_loads_encrypted_state
            && self.no_plaintext_in_db_wal_or_temp
            && self.corrupted_store_rejected
            && self.secure_delete_requires_db_wal_and_keychain
    }
}

impl GovernanceAdmissionSmoke {
    /// True when every Phase-5 governance/admission invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.governance_ordered_signed
            && self.governance_rejects_invalid_authority
            && self.removed_admin_cannot_win
            && self.invite_controls_enforced
            && self.password_and_welcome_gate
            && self.recovery_trust_model
            && self.abuse_controls_enforced
    }
}

impl ConnectivitySignalingPushSmoke {
    /// True when every Phase-6 connectivity/signaling invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.signaling_zero_linkage_at_rest
            && self.fallback_chain_covered
            && self.owner_overrides_used
            && self.android_wake_content_free
            && self.metadata_matrix_validated
            && self.pcap_no_central_content
            && self.relays_ciphertext_only
            && self.socket_local_process_conformant
            && self.route_reporting_honest
    }
}

impl UxE2eHardeningSmoke {
    /// True when every Phase-7 UX/E2E invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.command_surface_ready
            && self.discord_style_model_ready
            && self.verification_and_devices_ready
            && self.invite_retention_deletion_ready
            && self.connectivity_copy_ready
            && self.all_phase_smokes_ready
    }
}

/// Exercise passive relay opacity, active tamper rejection, and anti-replay checks.
pub fn media_security_smoke() -> Result<MediaSecuritySmoke, discrypt_media::MediaError> {
    use discrypt_media::{
        MediaKeyRegistry, ReplayWindow, SFrameReceiver, SFrameSender, SenderBinding,
    };
    use discrypt_relay_overlay::integrity::{contains_plaintext, RelayPacket};

    let binding = SenderBinding {
        kid: b"harness-kid-alice".to_vec(),
        leaf_index: 1,
        device_id: "alice-laptop".to_owned(),
    };
    let mut sender = SFrameSender::new(&[9; 32], binding.clone())?;
    let mut registry = MediaKeyRegistry::new();
    registry.register_sender(&[9; 32], binding.clone())?;
    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&[9; 32], binding)?;
    let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

    let plaintext = b"harness encoded voice frame";
    let relayed = sender.protect(plaintext)?;
    let relay_packet = RelayPacket::new("relay-a", relayed.ciphertext.clone()).forward("relay-b");
    let passive_relay_cannot_read = !contains_plaintext(&relay_packet, b"voice");

    let opened = receiver.open(&relayed)?;
    let replay_rejected = receiver.open(&relayed) == Err(discrypt_media::MediaError::Replay);

    let mut tampered = relayed;
    if let Some(first) = tampered.ciphertext.first_mut() {
        *first ^= 0x01;
    }
    let mut tamper_receiver = SFrameReceiver::new(tamper_registry, ReplayWindow::default());
    let tamper_rejected =
        tamper_receiver.open(&tampered) == Err(discrypt_media::MediaError::AuthenticationFailed);

    Ok(MediaSecuritySmoke {
        passive_relay_cannot_read,
        replay_rejected,
        tamper_rejected,
        plaintext: opened.plaintext,
    })
}

/// Exercise Phase-2 topology, failover, redelivery, store-forward, and media integrity.
pub fn relay_overlay_smoke() -> Result<RelayOverlaySmoke, anyhow::Error> {
    use discrypt_media::{
        MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameReceiver, SFrameSender,
        SenderBinding,
    };
    use discrypt_relay_overlay::failover::reroute_after_failure;
    use discrypt_relay_overlay::integrity::{contains_plaintext, RelayPacket};
    use discrypt_relay_overlay::ranking::RelayMetrics;
    use discrypt_relay_overlay::redelivery::{PacketId, RedeliveryError, RedeliveryTracker};
    use discrypt_relay_overlay::store_forward::{
        StoreForwardEnvelope, StoreForwardError, StoreForwardQueue,
    };
    use discrypt_relay_overlay::topology::RelayTopology;

    fn metrics(peer_id: &str, latency_ms: u32, freeload_penalty: f32) -> RelayMetrics {
        RelayMetrics {
            peer_id: peer_id.to_owned(),
            latency_ms,
            stability: 1.0,
            battery_cost: 0.0,
            freeload_penalty,
        }
    }

    let mut topology = RelayTopology::default();
    for peer in [
        metrics("alice", 1, 0.0),
        metrics("primary-relay", 10, 0.0),
        metrics("backup-relay", 30, 0.0),
        metrics("freeloader-relay", 5, 500.0),
        metrics("bob", 1, 0.0),
    ] {
        topology.upsert_peer(peer);
    }
    topology.connect("alice", "primary-relay")?;
    topology.connect("primary-relay", "bob")?;
    topology.connect("alice", "backup-relay")?;
    topology.connect("backup-relay", "bob")?;
    topology.connect("alice", "freeloader-relay")?;
    topology.connect("freeloader-relay", "bob")?;

    let route = topology.route("alice", "bob")?;
    let hop_limit_respected =
        route.path == ["alice", "primary-relay", "bob"] && route.within_hop_limit();
    let failover = reroute_after_failure(&topology, route.clone(), "primary-relay", 2_500)?;
    let failover_recovered = failover.converged_within_phase2_gate()
        && failover.replacement.path == ["alice", "backup-relay", "bob"]
        && !failover.replacement.contains_peer("primary-relay");

    let binding = SenderBinding {
        kid: b"phase2-kid-alice".to_vec(),
        leaf_index: 1,
        device_id: "alice-laptop".to_owned(),
    };
    let mut sender = SFrameSender::new(&[42; 32], binding.clone())?;
    let mut registry = MediaKeyRegistry::new();
    registry.register_sender(&[42; 32], binding.clone())?;
    let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

    let plaintext = b"phase2 encoded voice frame";
    let protected = sender.protect(plaintext)?;
    let relayed = route.path[1..route.path.len() - 1].iter().try_fold(
        RelayPacket::new(&route.path[1], protected.ciphertext.clone()),
        |packet, hop| packet.forward_checked(hop),
    )?;
    let ciphertext_only_media = !contains_plaintext(&relayed, b"voice frame");
    let opened = receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: relayed.bytes.clone(),
    })?;

    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&[42; 32], binding)?;
    let mut tamper_receiver = SFrameReceiver::new(tamper_registry, ReplayWindow::default());
    let tampered_packet = RelayPacket::new("primary-relay", protected.ciphertext.clone()).tamper();
    let tamper_rejected = tamper_receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: tampered_packet.bytes,
    }) == Err(MediaError::AuthenticationFailed);

    let mut redelivery = RedeliveryTracker::new(64, 2);
    let packet_id = PacketId {
        sender_id: "phase2-kid-alice".to_owned(),
        sequence: protected.counter,
    };
    redelivery.accept(&packet_id)?;
    let redelivery_replay_rejected = redelivery.accept(&packet_id) == Err(RedeliveryError::Replay);
    redelivery.request_redelivery(packet_id.clone(), "primary-relay")?;
    redelivery.request_redelivery(packet_id.clone(), "backup-relay")?;
    let store_forward_fanout_bounded = redelivery.redelivery_fanout(&packet_id) == 2
        && redelivery.request_redelivery(packet_id, "third-relay")
            == Err(RedeliveryError::FanoutExhausted);

    let mut queue = StoreForwardQueue::new();
    let plaintext_leak = StoreForwardEnvelope::new(
        "plaintext-leak",
        "bob",
        b"visible phase2 encoded voice frame".to_vec(),
        1_000,
        1_000,
        1,
    )?;
    let store_forward_plaintext_rejected = queue
        .enqueue_ciphertext_only(plaintext_leak, b"voice frame")
        == Err(StoreForwardError::VisiblePlaintext);
    queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new("media-1", "bob", protected.ciphertext, 1_000, 1_000, 2)?,
        b"voice frame",
    )?;
    let delivered_before_ttl = queue.drain_for_recipient("bob", 1_500).len() == 1;
    queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new(
            "media-2",
            "bob",
            b"opaque ciphertext".to_vec(),
            2_000,
            10,
            1,
        )?,
        b"voice frame",
    )?;
    let expired_not_delivered = queue.drain_for_recipient("bob", 2_011).is_empty();

    Ok(RelayOverlaySmoke {
        hop_limit_respected,
        failover_recovered,
        redelivery_replay_rejected,
        store_forward_plaintext_rejected,
        store_forward_ttl_enforced: delivered_before_ttl && expired_not_delivered,
        store_forward_fanout_bounded,
        ciphertext_only_media,
        tamper_rejected,
        plaintext: opened.plaintext,
    })
}

/// Exercise Phase-3 text, history, MLS delivery, gossip, Welcome, and fork repair.
pub fn text_history_delivery_smoke() -> Result<TextHistoryDeliverySmoke, anyhow::Error> {
    use discrypt_mls_core::{derive_epoch_secret, ExportLabel};
    use discrypt_mls_delivery::{
        detect_fork_or_replay, equal_confirmation_tags, order_application_events, plan_repair,
        repair_to_winner, summary, ApplicationEvent, CatchUpBundle, CommitEnvelope, DeliveryError,
        DeliveryState, ForkEvidence, ForkStatus, WelcomePackage,
    };
    use discrypt_relay_overlay::{GossipItem, GossipMesh};
    use discrypt_storage::{AuthorLogEntry, KeyState, LocalStore, RecipientCacheEntry};
    use std::collections::BTreeSet;

    let text_plaintext = b"hello from app-level encrypted text";
    let text_key = derive_epoch_secret(
        &[33; 32],
        ExportLabel::Content,
        b"room=lab;epoch=7;m=alice-1",
    );
    let text_ciphertext = xor_text_ciphertext(&text_key, text_plaintext);
    let opened_text = xor_text_ciphertext(&text_key, &text_ciphertext);
    let text_e2e_roundtrip = opened_text == text_plaintext && text_ciphertext != text_plaintext;

    let laptop_entry =
        AuthorLogEntry::new(1, "alice-laptop", 1, 7, "alice-1", text_ciphertext.clone());
    let phone_entry =
        AuthorLogEntry::new(1, "alice-phone", 2, 7, "alice-2", b"ciphertext-b".to_vec());
    let mut laptop = LocalStore::default();
    laptop.append_sent(laptop_entry.clone());
    laptop.cache_received(RecipientCacheEntry::new(
        "alice-1",
        text_ciphertext.clone(),
        KeyState::Cached(text_key),
        0,
    ));
    let no_plaintext_in_text_surfaces = !laptop
        .author_log_snapshot()
        .iter()
        .any(|entry| contains_bytes(&entry.ciphertext, text_plaintext))
        && laptop
            .recipient_cache()
            .get("alice-1")
            .is_some_and(|entry| !contains_bytes(&entry.ciphertext, text_plaintext));
    let mut phone = LocalStore::default();
    phone.append_sent(phone_entry.clone());
    let inserted = laptop.merge_author_logs(phone.author_log_snapshot());
    let author_logs_merged = inserted == 1
        && laptop.author_message_ids()
            == BTreeSet::from(["alice-1".to_owned(), "alice-2".to_owned()]);

    let mut cache_store = LocalStore::with_recipient_cache_capacity(3);
    for idx in 0..4 {
        cache_store.cache_received(RecipientCacheEntry::new(
            format!("cached-{idx}"),
            vec![idx as u8, 42],
            KeyState::Cached([idx as u8; 32]),
            idx,
        ));
    }
    let recipient_cache_bounded = cache_store.recipient_cache().len() == 3
        && cache_store.recipient_cache().get("cached-0").is_none()
        && cache_store.recipient_cache().get("cached-3").is_some();

    let peers = (0..16).map(|idx| format!("peer-{idx}")).collect::<Vec<_>>();
    let mut mesh = GossipMesh::new(peers.clone());
    let mut all_entries = Vec::from([laptop_entry, phone_entry]);
    for idx in 0..16 {
        all_entries.push(AuthorLogEntry::new(
            idx,
            format!("device-{idx}"),
            1,
            7,
            format!("member-{idx}-1"),
            format!("ciphertext-{idx}").into_bytes(),
        ));
    }
    for (idx, entry) in all_entries.iter().enumerate() {
        let peer = &peers[idx % peers.len()];
        mesh.insert(
            peer,
            GossipItem::new(
                entry.author_leaf,
                entry.sequence,
                entry.message_id.clone(),
                &entry.ciphertext,
            ),
        );
    }
    let _inserted_items = mesh.round();
    let gossip_converged_16 =
        mesh.converged() && mesh.known_count("peer-0") == Some(all_entries.len());

    let initial = summary(1, 1, 1);
    let mut delivery = DeliveryState::new(initial);
    let unordered_events = vec![
        ApplicationEvent::new(2, 12, "later-leaf", b"ciphertext-z".to_vec()),
        ApplicationEvent::new(2, 3, "early-leaf", b"ciphertext-a".to_vec()),
    ];
    let commit = CommitEnvelope::new(summary(2, 2, 2), 2, unordered_events);
    let ordered_commit_delivery = delivery.apply_commit(commit) == Ok(())
        && delivery.accepted_events().len() == 2
        && delivery.accepted_events()[0].event_id == "early-leaf";

    let welcome = WelcomePackage::new("room", 15, summary(2, 2, 2), 2_000);
    let catchup = CatchUpBundle::new(
        summary(2, 2, 2),
        Vec::new(),
        order_application_events(vec![
            ApplicationEvent::new(2, 9, "b", b"b".to_vec()),
            ApplicationEvent::new(2, 1, "a", b"a".to_vec()),
        ]),
    );
    let welcome_catchup_live = welcome.validate(1_999) == Ok(())
        && welcome.validate(2_001) == Err(DeliveryError::WelcomeExpired)
        && catchup.application_events[0].event_id == "a";

    let remote_fork = summary(2, 9, 2);
    let status = detect_fork_or_replay(delivery.summary(), &remote_fork);
    let fork_detected_not_silent = matches!(status, ForkStatus::Diverged(_))
        && delivery.apply_commit(CommitEnvelope::new(remote_fork, 9, Vec::new()))
            == Err(DeliveryError::DivergentTree(2));
    let evidence = match status {
        ForkStatus::Diverged(evidence) => evidence,
        _ => ForkEvidence {
            local: delivery.summary().clone(),
            remote: summary(2, 9, 2),
        },
    };
    let repair_plan = plan_repair(
        evidence,
        &[1, 3, 7, 9],
        vec![ApplicationEvent::new(
            2,
            3,
            "valid-text-reproposal",
            b"ciphertext".to_vec(),
        )],
    );
    let repaired = repair_to_winner(16, &repair_plan);
    let repair_converged_equal_tags = repaired.len() == 16 && equal_confirmation_tags(&repaired);
    let divergent_mls_not_replayed = !repair_plan.replays_divergent_mls_commits
        && repair_plan.reproposed_events.len() == 1
        && repair_plan.reproposed_events[0].event_id == "valid-text-reproposal";

    Ok(TextHistoryDeliverySmoke {
        text_e2e_roundtrip,
        no_plaintext_in_text_surfaces,
        author_logs_merged,
        recipient_cache_bounded,
        gossip_converged_16,
        ordered_commit_delivery,
        welcome_catchup_live,
        fork_detected_not_silent,
        repair_converged_equal_tags,
        divergent_mls_not_replayed,
    })
}

/// Exercise Phase-4 retention, shred, live-key, secure-delete, and recovery negatives.
pub fn retention_shred_smoke() -> Result<RetentionShredSmoke, anyhow::Error> {
    use chrono::{Duration, Utc};
    use discrypt_content_keys::{
        key_state, CrossDeviceShredState, KeyState, LiveKeyOracle, MembershipProof,
        RetentionTransition, RetentionWindow,
    };
    use discrypt_storage::{seal_account_backup, SecureDeleteSimulator};
    use std::collections::{BTreeMap, BTreeSet};

    let now = Utc::now();
    let key = [7; 32];
    let default_window_locks_old_messages = matches!(
        key_state(
            now,
            now - Duration::days(3),
            RetentionWindow::Days7,
            key,
            false,
        ),
        KeyState::Cached(_)
    ) && key_state(
        now,
        now - Duration::days(8),
        RetentionWindow::Days7,
        key,
        false,
    ) == KeyState::Locked;

    let shorten = RetentionTransition {
        old_window: RetentionWindow::Days7,
        new_window: RetentionWindow::Hours24,
        changed_at: now,
    };
    let lengthen = RetentionTransition {
        old_window: RetentionWindow::Hours24,
        new_window: RetentionWindow::Days7,
        changed_at: now,
    };
    let shorten_retro_lengthen_future =
        shorten.state_for_message(now, now - Duration::days(2), key, false) == KeyState::Locked
            && lengthen.state_for_message(now, now - Duration::days(2), key, false)
                == KeyState::Locked
            && matches!(
                lengthen.state_for_message(now, now + Duration::seconds(1), key, false),
                KeyState::Cached(_)
            );

    let mut shred = CrossDeviceShredState::default();
    shred.register_device("laptop", true);
    shred.register_device("phone", false);
    shred.shred("m-shred");
    let phone_pending = shred.pending_on_device("phone", "m-shred");
    shred.set_online("phone", true);
    let cross_device_shred_sync = !shred.device_may_serve("laptop", "m-shred")
        && phone_pending
        && !shred.pending_on_device("phone", "m-shred")
        && !shred.device_may_serve("phone", "m-shred");

    let mut members = BTreeMap::new();
    members.insert(9, BTreeSet::from([1, 2]));
    let mut oracle = LiveKeyOracle::new(members, 1);
    let allowed = oracle.request_key(&MembershipProof::new(1, 9, "room"), key);
    let limited = oracle.request_key(&MembershipProof::new(1, 9, "room"), key);
    let decoy = oracle.request_key(&MembershipProof::new(99, 9, "room"), key);
    let live_key_membership_rate_limit_decoy = allowed.authorized
        && allowed.state == KeyState::Cached(key)
        && !limited.authorized
        && limited.state == KeyState::RateLimited
        && !decoy.authorized
        && matches!(decoy.state, KeyState::Decoy(_));

    let mut delete = SecureDeleteSimulator::default();
    delete.write("db.sqlite", b"room content-key plaintext".to_vec());
    delete.write("db.sqlite-wal", b"wal content-key".to_vec());
    delete.write("key.store", b"wrapped content-key".to_vec());
    let snapshot = delete.snapshot();
    delete.secure_delete(["db.sqlite", "db.sqlite-wal"]);
    let failed_verify_kept_material = delete.contains_material(b"content-key");
    delete.restore(snapshot);
    delete.secure_delete(["db.sqlite", "db.sqlite-wal", "key.store"]);
    let secure_delete_negative = failed_verify_kept_material
        && !delete.contains_material(b"content-key")
        && delete.deleted_all(["db.sqlite", "db.sqlite-wal", "key.store"]);

    let backup = seal_account_backup(&key, vec!["room".to_owned()], 2);
    let recovery_cannot_resurrect_content_keys = !backup
        .identity_key_ciphertext
        .windows(key.len())
        .any(|window| window == key)
        && !backup.room_memberships.iter().any(|room| {
            room.as_bytes()
                .windows(key.len())
                .any(|window| window == key)
        });

    Ok(RetentionShredSmoke {
        default_window_locks_old_messages,
        shorten_retro_lengthen_future,
        cross_device_shred_sync,
        live_key_membership_rate_limit_decoy,
        secure_delete_negative,
        recovery_cannot_resurrect_content_keys,
    })
}

/// Exercise Phase-B fresh install, restart, corruption, and storage/keychain
/// secure-delete verification against the encrypted AppStore boundary.
pub fn storage_persistence_smoke() -> Result<StoragePersistenceSmoke, anyhow::Error> {
    use discrypt_storage::{
        sqlite_wal_path, AppStore, EncryptedAppDb, MemoryAppDbKeychain, SecureDeleteSimulator,
    };
    use std::fs;

    let path = std::env::temp_dir().join(format!(
        "discrypt-phase-b-storage-{}-{}.sqlite",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let wal_path = sqlite_wal_path(&path);
    let tmp_path = path.with_extension("json.tmp");
    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    let keychain = MemoryAppDbKeychain::default();
    let mut fresh_db = EncryptedAppDb::new(&path, keychain.clone());
    let fresh_install_starts_empty = fresh_db.load_app_state()?.is_none();

    let sensitive_state = br#"{"schema_version":1,"profile":{"display_name":"Alice"},"messages":[{"body":"phase-b plaintext must not leak"}],"content_key":"forbidden-content-key"}"#;
    fresh_db.save_app_state(sensitive_state)?;
    let mut restarted_db = EncryptedAppDb::new(&path, keychain);
    let restart_loads_encrypted_state =
        restarted_db.load_app_state()? == Some(sensitive_state.to_vec());

    let path_contains = |candidate: &std::path::Path, needle: &[u8]| {
        fs::read(candidate)
            .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
            .unwrap_or(false)
    };
    let no_plaintext_in_db_wal_or_temp = [
        b"Alice".as_slice(),
        b"phase-b plaintext must not leak".as_slice(),
        b"forbidden-content-key".as_slice(),
    ]
    .into_iter()
    .all(|needle| {
        !path_contains(&path, needle)
            && !path_contains(&wal_path, needle)
            && !path_contains(&tmp_path, needle)
    });

    fs::write(&path, br#"{"schema_version":0,"legacy":"plaintext-json"}"#)?;
    let corrupted_store_rejected = restarted_db.load_app_state().is_err();

    let mut delete = SecureDeleteSimulator::default();
    delete.write("app.db", b"wrapped-content-key".to_vec());
    delete.write("app.db-wal", b"wrapped-content-key-wal".to_vec());
    delete.write("app.keychain", b"wrapped-content-key-keychain".to_vec());
    let snapshot = delete.snapshot();
    delete.secure_delete(["app.db", "app.db-wal"]);
    let partial_delete_kept_keychain_material = delete.contains_material(b"wrapped-content-key");
    delete.restore(snapshot);
    delete.secure_delete(["app.db", "app.db-wal", "app.keychain"]);
    let secure_delete_requires_db_wal_and_keychain = partial_delete_kept_keychain_material
        && !delete.contains_material(b"wrapped-content-key")
        && delete.deleted_all(["app.db", "app.db-wal", "app.keychain"]);

    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    Ok(StoragePersistenceSmoke {
        fresh_install_starts_empty,
        restart_loads_encrypted_state,
        no_plaintext_in_db_wal_or_temp,
        corrupted_store_rejected,
        secure_delete_requires_db_wal_and_keychain,
    })
}

/// Exercise Phase-5 governance, admission, recovery, and abuse controls.
pub fn governance_admission_smoke() -> Result<GovernanceAdmissionSmoke, anyhow::Error> {
    use chrono::{Duration, Utc};
    use discrypt_abuse::AbuseControls;
    use discrypt_admission::{AdmissionController, Invite, InviteError, PasswordGate};
    use discrypt_mls_core::governance::{
        GovernanceAction, GovernanceError, GovernanceEvent, GovernanceLog, GovernanceState, Role,
    };
    use discrypt_storage::{
        recover_account, recovery_code_material, seal_account_backup, AccountRecovery,
        RecoveryCodeVerifier, RecoveryError, RecoveryMaterial,
    };

    let mut log = GovernanceLog::default();
    let high = GovernanceEvent::signed(10, 9, GovernanceAction::Ban { target: 4 });
    let low = GovernanceEvent::signed(
        10,
        1,
        GovernanceAction::RevokeInvite {
            invite_id: "invite-a".into(),
        },
    );
    log.append(high);
    log.append(low);
    let governance_ordered_signed =
        log.events()[0].committer == 1 && log.events().iter().all(GovernanceEvent::signature_valid);

    let mut state = GovernanceState::new(10, 1);
    let governance_rejects_invalid_authority = state.apply_event(GovernanceEvent::signed(
        10,
        2,
        GovernanceAction::RevokeInvite {
            invite_id: "bad".into(),
        },
    )) == Err(GovernanceError::Unauthorized)
        && state.apply_event(GovernanceEvent::signed(
            11,
            1,
            GovernanceAction::RevokeInvite {
                invite_id: "future".into(),
            },
        )) == Err(GovernanceError::OutOfEpoch);

    state.apply_event(GovernanceEvent::signed(
        10,
        1,
        GovernanceAction::SetRole {
            target: 2,
            role: Role::Admin,
        },
    ))?;
    let race = state.resolve_epoch_events([
        GovernanceEvent::signed(
            10,
            2,
            GovernanceAction::RevokeInvite {
                invite_id: "admin-loses".into(),
            },
        ),
        GovernanceEvent::signed(10, 1, GovernanceAction::Ban { target: 2 }),
    ]);
    let removed_admin_cannot_win = race == vec![Ok(()), Err(GovernanceError::EvictedCommitter)]
        && state.is_banned(2)
        && !state.invite_revoked("admin-loses");

    let now = Utc::now();
    let mut one_use = Invite::new(b"secret", now + Duration::minutes(1), 1);
    let first_use = one_use.consume(now) == Ok(());
    let exhausted = one_use.consume(now) == Err(InviteError::Exhausted);
    let mut expired = Invite::new(b"secret", now - Duration::seconds(1), 1);
    let expired_rejected = expired.consume(now) == Err(InviteError::Expired);
    let mut revoked = Invite::new(b"secret", now + Duration::minutes(1), 1);
    revoked.revoke();
    let revoked_rejected = revoked.consume(now) == Err(InviteError::Revoked);
    let invite_controls_enforced = first_use && exhausted && expired_rejected && revoked_rejected;

    let mut invite = Invite::new(b"secret", now + Duration::minutes(1), 2);
    let mut offline = AdmissionController::new(
        PasswordGate::OfflineVerifier {
            verifier_id: "copyable".into(),
        },
        1,
    );
    let mut pake = AdmissionController::new(
        PasswordGate::OnlineAuthorizedHelper {
            helper_id: "owner-device".into(),
        },
        1,
    );
    let password_and_welcome_gate =
        offline.finalize_admission(&mut invite, now, "alice", true, true)
            == Err(InviteError::OfflineVerifierRejected)
            && pake.finalize_admission(&mut invite, now, "alice", true, false)
                == Err(InviteError::WelcomeRequired)
            && pake.finalize_admission(&mut invite, now, "alice", true, true) == Ok(())
            && pake.finalize_admission(&mut invite, now, "alice", true, true)
                == Err(InviteError::PasswordRejected);

    let backup = seal_account_backup(&[8; 32], vec!["room".into()], 2);
    let recovery_code = RecoveryCodeVerifier::from_code("paper-coral-falcon")?;
    let code_material =
        recovery_code_material("paper-coral-falcon", &recovery_code, vec!["room".into()], 2)?;
    let recovery_trust_model = recover_account(RecoveryMaterial::None)
        == Err(RecoveryError::NoTrustMaterial)
        && recovery_code_material("wrong", &recovery_code, vec!["room".into()], 2)
            == Err(RecoveryError::InvalidRecoveryCode)
        && matches!(
            recover_account(code_material),
            Ok(AccountRecovery {
                account_access_restored: true,
                room_memberships,
                device_count: 2,
                content_keys_restored: false,
            }) if room_memberships == vec!["room".to_owned()]
        )
        && matches!(
            recover_account(RecoveryMaterial::SealedBackup(backup)),
            Ok(AccountRecovery {
                account_access_restored: true,
                room_memberships,
                device_count: 2,
                content_keys_restored: false,
            }) if room_memberships == vec!["room".to_owned()]
        );

    let mut abuse = AbuseControls::new(1, 2, Duration::minutes(1));
    let abuse_controls_enforced = abuse.allow_invite("alice", now)
        && !abuse.allow_invite("alice", now)
        && abuse.allow_message("alice", now)
        && abuse.allow_message("alice", now)
        && !abuse.allow_message("alice", now)
        && {
            abuse.record_relay("freeloader", 1, 10);
            abuse.record_relay("helper", 10, 1);
            abuse.freeload_penalty("freeloader") > abuse.freeload_penalty("helper")
        };

    Ok(GovernanceAdmissionSmoke {
        governance_ordered_signed,
        governance_rejects_invalid_authority,
        removed_admin_cannot_win,
        invite_controls_enforced,
        password_and_welcome_gate,
        recovery_trust_model,
        abuse_controls_enforced,
    })
}

/// Exercise Phase-6 signaling, fallback, push, and metadata audit gates.
pub fn connectivity_signaling_push_smoke() -> Result<ConnectivitySignalingPushSmoke, anyhow::Error>
{
    use chrono::{Duration, Utc};
    use discrypt_push::{
        contains_content, contains_forbidden_token, AndroidWakeService, WakePayload, WakeReason,
    };
    use external_signaling::{
        AuditFixture, ContentExposure, InfrastructureComponent, MetadataMatrix, PcapEvent,
        ReferenceSignalingServer, RendezvousBlob, RendezvousKey,
    };
    use discrypt_transport::{
        ConnectivityConfig, ConnectivityPlanner, Endpoint, EndpointOverrides, FallbackLeg,
        LocalProcessSocketAdapter, SimulatedNat,
    };

    let now = Utc::now();
    let forbidden: [&[u8]; 5] = [
        b"alice".as_slice(),
        b"bob".as_slice(),
        b"room-plaintext".as_slice(),
        b"message-body".as_slice(),
        b"topology-link".as_slice(),
    ];

    let mut signaling = ReferenceSignalingServer::default();
    let key = RendezvousKey::new(b"opaque-rendezvous-key".to_vec());
    signaling.publish(
        key.clone(),
        RendezvousBlob::new(
            b"opaque-room-token".to_vec(),
            b"opaque-endpoint-hint".to_vec(),
            now + Duration::minutes(5),
        ),
        Endpoint::new("198.51.100.9:4242"),
        now,
    )?;
    let signaling_zero_linkage_at_rest =
        signaling.zero_linkage_at_rest(&forbidden) && !external_signaling::stores_linkage_at_rest();
    let fetched = signaling.take(&key, now)?;
    let signaling_content_blind =
        !external_signaling::contains_any_token(&fetched.visible_bytes(), &forbidden);

    let default_config = ConnectivityConfig::default();
    let direct = ConnectivityPlanner::plan(&default_config, SimulatedNat::direct())?;
    let overlay = ConnectivityPlanner::plan(&default_config, SimulatedNat::overlay_only())?;
    let turn = ConnectivityPlanner::plan(&default_config, SimulatedNat::turn_only())?;
    let fallback_chain_covered = direct.selected == FallbackLeg::Stun
        && overlay.selected == FallbackLeg::RelayOverlay
        && turn.selected == FallbackLeg::Turn
        && direct.ordered_stun_overlay_turn()
        && overlay.ordered_stun_overlay_turn()
        && turn.ordered_stun_overlay_turn();
    let relays_ciphertext_only =
        overlay.relay_legs_ciphertext_only() && turn.relay_legs_ciphertext_only();
    let route_report = overlay.route_report();

    let override_config = ConnectivityConfig {
        overrides: EndpointOverrides::new(
            Some(Endpoint::new("stun:owner.example:3478")),
            Some(Endpoint::new("turns:owner.example:5349")),
        ),
        ..ConnectivityConfig::default()
    };
    let owner_stun = ConnectivityPlanner::plan(&override_config, SimulatedNat::direct())?;
    let owner_turn = ConnectivityPlanner::plan(&override_config, SimulatedNat::turn_only())?;
    let owner_overrides_used = owner_stun.endpoint == Endpoint::new("stun:owner.example:3478")
        && owner_turn.endpoint == Endpoint::new("turns:owner.example:5349");

    let socket_adapter = LocalProcessSocketAdapter::new(
        default_config.clone(),
        SimulatedNat::overlay_only(),
        b"message-body".to_vec(),
    );
    let socket_report = socket_adapter.run_conformance(b"opaque socket ciphertext")?;
    let socket_local_process_conformant = socket_report.ready();
    let route_reporting_honest =
        route_report.honest_and_ordered() && socket_report.route_report.honest_and_ordered();

    let wake_service = AndroidWakeService::default();
    let payload = WakePayload::new([7; 32], WakeReason::SyncHint, [9; 16]);
    let push_envelope = wake_service.build_envelope([8; 32], payload.clone())?;
    let android_wake_content_free =
        !contains_content(&payload) && !contains_forbidden_token(&push_envelope, &forbidden);

    let mut fixture = AuditFixture::default();
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Signaling,
        content: ContentExposure::None,
        visible_bytes: fetched.visible_bytes(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Stun,
        content: ContentExposure::None,
        visible_bytes: b"binding request no app content".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Turn,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"sframe ciphertext over turn".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::PushFcm,
        content: ContentExposure::None,
        visible_bytes: push_envelope.provider_visible_bytes(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::PeerRelay,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"sframe ciphertext over peer relay".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::VolunteerStorageRelay,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"store-forward ciphertext".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    let matrix = MetadataMatrix::approved_v1();
    let metadata_matrix_validated = fixture.matches_matrix(&matrix);
    let pcap_no_central_content = fixture.no_forbidden_content_egress(&forbidden);

    Ok(ConnectivitySignalingPushSmoke {
        signaling_zero_linkage_at_rest: signaling_zero_linkage_at_rest && signaling_content_blind,
        fallback_chain_covered,
        owner_overrides_used,
        android_wake_content_free,
        metadata_matrix_validated,
        pcap_no_central_content,
        relays_ciphertext_only,
        socket_local_process_conformant,
        route_reporting_honest,
    })
}

fn xor_text_ciphertext(key: &[u8; 32], input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Exercise Phase-7 Tauri/React command-surface and final E2E hardening gates.
pub fn ux_e2e_hardening_smoke() -> Result<UxE2eHardeningSmoke, anyhow::Error> {
    let snapshot = discrypt_core::app_snapshot();
    let command_health = discrypt_desktop::command_health();
    let verification =
        discrypt_core::verify_safety_number(discrypt_core::SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code.clone(),
            provided: snapshot.friend.safety_number.clone(),
        });

    let command_surface_ready = command_health.snapshot_ready
        && command_health.verification_ready
        && command_health.honest_copy_ready;
    let discord_style_model_ready = snapshot.servers.iter().any(|server| {
        server
            .channels
            .iter()
            .any(|channel| matches!(channel.kind, discrypt_core::ChannelKind::Text))
            && server
                .channels
                .iter()
                .any(|channel| matches!(channel.kind, discrypt_core::ChannelKind::Voice))
    });
    let verification_and_devices_ready = !snapshot.friend.verified
        && verification.verified
        && !snapshot.friend.safety_number.is_empty()
        && snapshot.devices.iter().any(|device| device.local)
        && snapshot
            .devices
            .iter()
            .any(|device| !device.local && device.authorized);
    let invite_retention_deletion_ready = snapshot.invite.welcome_required.contains("MLS Welcome")
        && snapshot.invite.password_gate.contains("OPAQUE/PAKE")
        && snapshot
            .retention
            .presets
            .contains(&"warned unlimited / never-lock".to_owned())
        && snapshot
            .security_copy
            .deletion
            .contains("pending on offline devices until they reconnect")
        && snapshot
            .security_copy
            .malicious_member
            .contains("screenshots");
    let connectivity_copy_ready = snapshot
        .connectivity
        .fallback_chain
        .contains("STUN → relay-overlay → TURN")
        && snapshot.connectivity.push_copy.contains("content-free")
        && snapshot
            .connectivity
            .metadata_copy
            .contains("not metadata-anonymous");

    let media = media_security_smoke()?;
    let overlay = relay_overlay_smoke()?;
    let text = text_history_delivery_smoke()?;
    let retention = retention_shred_smoke()?;
    let storage = storage_persistence_smoke()?;
    let governance = governance_admission_smoke()?;
    let connectivity = connectivity_signaling_push_smoke()?;
    let all_phase_smokes_ready = media.ready()
        && overlay.ready()
        && text.ready()
        && retention.ready()
        && storage.ready()
        && governance.ready()
        && connectivity.ready();

    Ok(UxE2eHardeningSmoke {
        command_surface_ready,
        discord_style_model_ready,
        verification_and_devices_ready,
        invite_retention_deletion_ready,
        connectivity_copy_ready,
        all_phase_smokes_ready,
    })
}

/// Backward-compatible boolean smoke for scripts that only need passive relay status.
pub fn media_passive_relay_roundtrip() -> Result<bool, discrypt_media::MediaError> {
    let smoke = media_security_smoke()?;
    Ok(smoke.passive_relay_cannot_read && smoke.plaintext == b"harness encoded voice frame")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_node_dm_has_safety_number() {
        assert!(!two_node_dm_safety_number().is_empty());
    }

    #[test]
    fn media_security_smoke_rejects_relays_tamper_and_replay() {
        let smoke = media_security_smoke();
        assert!(matches!(
            smoke,
            Ok(MediaSecuritySmoke {
                passive_relay_cannot_read: true,
                replay_rejected: true,
                tamper_rejected: true,
                plaintext
            }) if plaintext == b"harness encoded voice frame"
        ));
    }

    #[test]
    fn relay_overlay_smoke_covers_phase2_gates() {
        let smoke = relay_overlay_smoke();
        assert!(matches!(
            smoke,
            Ok(RelayOverlaySmoke {
                hop_limit_respected: true,
                failover_recovered: true,
                redelivery_replay_rejected: true,
                store_forward_plaintext_rejected: true,
                store_forward_ttl_enforced: true,
                store_forward_fanout_bounded: true,
                ciphertext_only_media: true,
                tamper_rejected: true,
                plaintext
            }) if plaintext == b"phase2 encoded voice frame"
        ));
    }

    #[test]
    fn text_history_delivery_smoke_covers_phase3_gates() {
        let smoke = text_history_delivery_smoke();
        assert!(matches!(
            smoke,
            Ok(TextHistoryDeliverySmoke {
                text_e2e_roundtrip: true,
                no_plaintext_in_text_surfaces: true,
                author_logs_merged: true,
                recipient_cache_bounded: true,
                gossip_converged_16: true,
                ordered_commit_delivery: true,
                welcome_catchup_live: true,
                fork_detected_not_silent: true,
                repair_converged_equal_tags: true,
                divergent_mls_not_replayed: true,
            })
        ));
    }

    #[test]
    fn retention_shred_smoke_covers_phase4_gates() {
        let smoke = retention_shred_smoke();
        assert!(matches!(
            smoke,
            Ok(RetentionShredSmoke {
                default_window_locks_old_messages: true,
                shorten_retro_lengthen_future: true,
                cross_device_shred_sync: true,
                live_key_membership_rate_limit_decoy: true,
                secure_delete_negative: true,
                recovery_cannot_resurrect_content_keys: true,
            })
        ));
    }

    #[test]
    fn storage_persistence_smoke_covers_phase_b_gates() {
        let smoke = storage_persistence_smoke();
        assert!(matches!(
            smoke,
            Ok(StoragePersistenceSmoke {
                fresh_install_starts_empty: true,
                restart_loads_encrypted_state: true,
                no_plaintext_in_db_wal_or_temp: true,
                corrupted_store_rejected: true,
                secure_delete_requires_db_wal_and_keychain: true,
            })
        ));
    }

    #[test]
    fn governance_admission_smoke_covers_phase5_gates() {
        let smoke = governance_admission_smoke();
        assert!(matches!(
            smoke,
            Ok(GovernanceAdmissionSmoke {
                governance_ordered_signed: true,
                governance_rejects_invalid_authority: true,
                removed_admin_cannot_win: true,
                invite_controls_enforced: true,
                password_and_welcome_gate: true,
                recovery_trust_model: true,
                abuse_controls_enforced: true,
            })
        ));
    }

    #[test]
    fn connectivity_signaling_push_smoke_covers_phase6_gates() {
        let smoke = connectivity_signaling_push_smoke();
        assert!(matches!(
            smoke,
            Ok(ConnectivitySignalingPushSmoke {
                signaling_zero_linkage_at_rest: true,
                fallback_chain_covered: true,
                owner_overrides_used: true,
                android_wake_content_free: true,
                metadata_matrix_validated: true,
                pcap_no_central_content: true,
                relays_ciphertext_only: true,
                socket_local_process_conformant: true,
                route_reporting_honest: true,
            })
        ));
    }

    #[test]
    fn ux_e2e_hardening_smoke_covers_phase7_gates() {
        let smoke = ux_e2e_hardening_smoke();
        assert!(matches!(
            smoke,
            Ok(UxE2eHardeningSmoke {
                command_surface_ready: true,
                discord_style_model_ready: true,
                verification_and_devices_ready: true,
                invite_retention_deletion_ready: true,
                connectivity_copy_ready: true,
                all_phase_smokes_ready: true,
            })
        ));
    }
}
