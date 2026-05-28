//! Domain orchestration facade for Tauri commands.
use admission::Invite;
use mls_core::{GroupState, Identity};
use serde::{Deserialize, Serialize};

/// Room summary returned to UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoomSummary {
    pub room_id: String,
    pub epoch: u64,
    pub members: usize,
}
#[must_use]
pub fn create_dm(alice: &Identity, bob: &Identity) -> (GroupState, String) {
    let group = GroupState::new(format!(
        "dm:{}:{}",
        alice.friend_code().as_str(),
        bob.friend_code().as_str()
    ));
    let safety = alice
        .safety_number(&bob.verifying_key())
        .as_str()
        .to_owned();
    (group, safety)
}
#[must_use]
pub fn summarize(group: &GroupState) -> RoomSummary {
    RoomSummary {
        room_id: group.group_id.clone(),
        epoch: group.epoch,
        members: group.members().len(),
    }
}
#[allow(dead_code)]
fn _invite_boundary(_: &Invite) {}
