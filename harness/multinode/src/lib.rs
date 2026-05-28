//! Headless multinode harness for discrypt acceptance tests.
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
    use discrypt_mls_delivery::{
        detect_fork_or_replay, equal_confirmation_tags, order_application_events, plan_repair,
        repair_to_winner, summary, ApplicationEvent, CatchUpBundle, CommitEnvelope, DeliveryError,
        DeliveryState, ForkEvidence, ForkStatus, WelcomePackage,
    };
    use discrypt_relay_overlay::{GossipItem, GossipMesh};
    use discrypt_storage::{AuthorLogEntry, KeyState, LocalStore, RecipientCacheEntry};
    use std::collections::BTreeSet;

    let laptop_entry =
        AuthorLogEntry::new(1, "alice-laptop", 1, 7, "alice-1", b"ciphertext-a".to_vec());
    let phone_entry =
        AuthorLogEntry::new(1, "alice-phone", 2, 7, "alice-2", b"ciphertext-b".to_vec());
    let mut laptop = LocalStore::default();
    laptop.append_sent(laptop_entry.clone());
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
}
