use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::{
    media_security_smoke, relay_overlay_smoke, retention_shred_smoke, text_history_delivery_smoke,
};

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match (
        media_security_smoke(),
        relay_overlay_smoke(),
        text_history_delivery_smoke(),
        retention_shred_smoke(),
    ) {
        (Ok(media), Ok(overlay), Ok(text), Ok(retention)) => println!(
            "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={} overlay_hops={} overlay_failover={} overlay_redelivery={} overlay_store_forward_plaintext={} overlay_store_forward_ttl={} overlay_store_forward_fanout={} overlay_ciphertext_only={} overlay_tamper={} text_author_logs={} text_cache_bounded={} text_gossip16={} text_ordered_delivery={} text_welcome_catchup={} text_fork_detected={} text_repair_converged={} text_no_mls_replay={} retention_default_lock={} retention_transition={} shred_cross_device={} live_key_oracle={} secure_delete={} recovery_no_content_keys={}",
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
            retention.recovery_cannot_resurrect_content_keys
        ),
        (Err(error), _, _, _) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
        (_, Err(error), _, _) => {
            eprintln!("discrypt multinode harness relay overlay smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, Err(error), _) => {
            eprintln!("discrypt multinode harness text history smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, Err(error)) => {
            eprintln!("discrypt multinode harness retention/shred smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
