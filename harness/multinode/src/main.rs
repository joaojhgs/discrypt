use discrypt_core::summarize;
use discrypt_mls_core::GroupState;

fn main() {
    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    println!(
        "discrypt multinode harness: room={} epoch={} members={}",
        summary.room_id, summary.epoch, summary.members
    );
}
