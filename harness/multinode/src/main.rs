use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::{media_security_smoke, relay_overlay_smoke};

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match (media_security_smoke(), relay_overlay_smoke()) {
        (Ok(media), Ok(overlay)) => println!(
            "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={} overlay_hops={} overlay_failover={} overlay_redelivery={} overlay_store_forward_plaintext={} overlay_store_forward_ttl={} overlay_store_forward_fanout={} overlay_ciphertext_only={} overlay_tamper={}",
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
            overlay.tamper_rejected
        ),
        (Err(error), _) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
        (_, Err(error)) => {
            eprintln!("discrypt multinode harness relay overlay smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
