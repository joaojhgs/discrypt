use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::media_security_smoke;

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match media_security_smoke() {
        Ok(media) => println!(
            "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={}",
            summary.room_id,
            summary.epoch,
            summary.members,
            media.passive_relay_cannot_read,
            media.replay_rejected,
            media.tamper_rejected
        ),
        Err(error) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
