use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::{
    connectivity_signaling_push_smoke, governance_admission_smoke, media_security_smoke,
    relay_overlay_smoke, retention_shred_smoke, text_history_delivery_smoke,
};

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match (
        media_security_smoke(),
        relay_overlay_smoke(),
        text_history_delivery_smoke(),
        retention_shred_smoke(),
        governance_admission_smoke(),
        connectivity_signaling_push_smoke(),
    ) {
        (Ok(media), Ok(overlay), Ok(text), Ok(retention), Ok(governance), Ok(connectivity)) => println!(
            "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={} overlay_hops={} overlay_failover={} overlay_redelivery={} overlay_store_forward_plaintext={} overlay_store_forward_ttl={} overlay_store_forward_fanout={} overlay_ciphertext_only={} overlay_tamper={} text_author_logs={} text_cache_bounded={} text_gossip16={} text_ordered_delivery={} text_welcome_catchup={} text_fork_detected={} text_repair_converged={} text_no_mls_replay={} retention_default_lock={} retention_transition={} shred_cross_device={} live_key_oracle={} secure_delete={} recovery_no_content_keys={} gov_ordered={} gov_invalid_rejected={} gov_removed_admin={} admission_invites={} admission_password_welcome={} recovery_trust={} abuse_controls={} signaling_zero_linkage={} connectivity_fallback={} connectivity_overrides={} android_wake_content_free={} metadata_matrix={} pcap_no_content={} relays_ciphertext_only={}",
            summary.room_id,
            summary.epoch,
            summary.members,
            media.passive_relay_cannot_read,
            media.replay_rejected,
            media.tamper_rejected,
            overlay.hop_limit_respected,
            overlay.failover_recovered,
            overlay.redelivery_replay_rejected,
            overlay.store_forward_plaintext_rejected,
            overlay.store_forward_ttl_enforced,
            overlay.store_forward_fanout_bounded,
            overlay.ciphertext_only_media,
            overlay.tamper_rejected,
            text.author_logs_merged,
            text.recipient_cache_bounded,
            text.gossip_converged_16,
            text.ordered_commit_delivery,
            text.welcome_catchup_live,
            text.fork_detected_not_silent,
            text.repair_converged_equal_tags,
            text.divergent_mls_not_replayed,
            retention.default_window_locks_old_messages,
            retention.shorten_retro_lengthen_future,
            retention.cross_device_shred_sync,
            retention.live_key_membership_rate_limit_decoy,
            retention.secure_delete_negative,
            retention.recovery_cannot_resurrect_content_keys,
            governance.governance_ordered_signed,
            governance.governance_rejects_invalid_authority,
            governance.removed_admin_cannot_win,
            governance.invite_controls_enforced,
            governance.password_and_welcome_gate,
            governance.recovery_trust_model,
            governance.abuse_controls_enforced,
            connectivity.signaling_zero_linkage_at_rest,
            connectivity.fallback_chain_covered,
            connectivity.owner_overrides_used,
            connectivity.android_wake_content_free,
            connectivity.metadata_matrix_validated,
            connectivity.pcap_no_central_content,
            connectivity.relays_ciphertext_only
        ),
        (Err(error), _, _, _, _, _) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
        (_, Err(error), _, _, _, _) => {
            eprintln!("discrypt multinode harness relay overlay smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, Err(error), _, _, _) => {
            eprintln!("discrypt multinode harness text history smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, Err(error), _, _) => {
            eprintln!("discrypt multinode harness retention/shred smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, Err(error), _) => {
            eprintln!("discrypt multinode harness governance/admission smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, _, Err(error)) => {
            eprintln!("discrypt multinode harness connectivity/signaling/push smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
