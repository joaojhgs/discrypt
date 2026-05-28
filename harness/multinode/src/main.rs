use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::{
    media_security_smoke, relay_overlay_smoke, text_history_delivery_smoke,
};

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match (
        media_security_smoke(),
        relay_overlay_smoke(),
        text_history_delivery_smoke(),
    ) {
        (Ok(media), Ok(overlay), Ok(text)) => println!(
            "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={} overlay_hops={} overlay_failover={} overlay_redelivery={} overlay_store_forward_plaintext={} overlay_store_forward_ttl={} overlay_store_forward_fanout={} overlay_ciphertext_only={} overlay_tamper={} text_author_logs={} text_cache_bounded={} text_gossip16={} text_ordered_delivery={} text_welcome_catchup={} text_fork_detected={} text_repair_converged={} text_no_mls_replay={}",
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
            text.divergent_mls_not_replayed
        ),
        (Err(error), _, _) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
        (_, Err(error), _) => {
            eprintln!("discrypt multinode harness relay overlay smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, Err(error)) => {
            eprintln!("discrypt multinode harness text history smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
