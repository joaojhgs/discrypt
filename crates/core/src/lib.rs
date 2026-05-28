//! Domain orchestration facade for Tauri commands and headless E2E tests.
use admission::Invite;
use mls_core::{GroupState, Identity};
use serde::{Deserialize, Serialize};

/// Room summary returned to UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoomSummary {
    /// Stable room identifier.
    pub room_id: String,
    /// Current MLS epoch facade.
    pub epoch: u64,
    /// Current member count facade.
    pub members: usize,
}

/// Friend/safety-number row for verification UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FriendView {
    /// Local display label.
    pub alias: String,
    /// Friend-code/QR payload preview.
    pub friend_code: String,
    /// Pairwise safety number that must be verified out of band.
    pub safety_number: String,
    /// Whether the user has explicitly verified this safety number.
    pub verified: bool,
}

/// Device-management row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeviceView {
    /// Device id shown in transparency notices.
    pub device_id: String,
    /// MLS leaf index for this device.
    pub leaf_index: u32,
    /// Whether this device is current/local.
    pub local: bool,
    /// Whether this device is authorized by an existing device.
    pub authorized: bool,
}

/// Discord-style server/channel row.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ChannelView {
    /// Channel name.
    pub name: String,
    /// Channel kind.
    pub kind: ChannelKind,
    /// Retention status for channel history.
    pub retention_status: String,
}

/// UI channel kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChannelKind {
    /// Text channel.
    Text,
    /// Voice channel.
    Voice,
}

/// Discord-style server view.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServerView {
    /// Server/room label.
    pub name: String,
    /// Channels visible in the sidebar.
    pub channels: Vec<ChannelView>,
    /// Role label from governance state.
    pub role: String,
}

/// Invite flow status shown to users/admins.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InviteFlowView {
    /// Invite expiry copy.
    pub expires: String,
    /// Max-use copy.
    pub max_use: String,
    /// Password-gate posture copy.
    pub password_gate: String,
    /// Final MLS Welcome/add gate copy.
    pub welcome_required: String,
}

/// Retention settings shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RetentionSettingsView {
    /// Available retention presets.
    pub presets: Vec<String>,
    /// Current default setting.
    pub selected: String,
    /// Warning for unlimited/never-lock.
    pub unlimited_warning: String,
    /// Shorten/lengthen semantic copy.
    pub transition_copy: String,
}

/// Voice-room status shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VoiceRoomView {
    /// Current route label.
    pub route: String,
    /// Relay security copy.
    pub relay_copy: String,
    /// Android path copy.
    pub android_path: String,
}

/// Connectivity status shown in UX.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ConnectivityView {
    /// Ordered fallback chain copy.
    pub fallback_chain: String,
    /// Metadata posture copy.
    pub metadata_copy: String,
    /// Push wake posture copy.
    pub push_copy: String,
}

/// Required security/deletion copy surfaced in release UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SecurityCopyView {
    /// Metadata caveat.
    pub metadata: String,
    /// Cooperative deletion caveat.
    pub deletion: String,
    /// Malicious-recipient caveat.
    pub malicious_member: String,
}

/// Snapshot returned by the Tauri command surface and consumed by the React shell.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppSnapshot {
    /// Serialized command contract schema version.
    pub schema_version: u32,
    /// Friend and explicit verification state.
    pub friend: FriendView,
    /// Device-management rows.
    pub devices: Vec<DeviceView>,
    /// Server/channel navigation rows.
    pub servers: Vec<ServerView>,
    /// Invite/admission flow copy.
    pub invite: InviteFlowView,
    /// Retention settings model.
    pub retention: RetentionSettingsView,
    /// Voice-room status.
    pub voice: VoiceRoomView,
    /// Connectivity/signaling/push posture.
    pub connectivity: ConnectivityView,
    /// Mandatory security copy.
    pub security_copy: SecurityCopyView,
}

/// Safety-number verification request from UI.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyVerificationRequest {
    /// Friend/device identifier whose expected safety number is backend-owned.
    pub friend_id: String,
    /// User-confirmed safety number from QR/out-of-band comparison.
    pub provided: String,
}

/// Safety-number verification response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SafetyVerificationResult {
    /// Whether the values match exactly.
    pub verified: bool,
    /// User-facing result copy.
    pub message: String,
}

const SNAPSHOT_SCHEMA_VERSION: u32 = 1;
const FIXTURE_BOB_FRIEND_CODE: &str = "friend:bob:stable-fixture";
const FIXTURE_BOB_SAFETY_NUMBER: &str = "0231 1597 2653 5897";

/// Create a deterministic DM facade and safety number.
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

/// Summarize group state for UI.
#[must_use]
pub fn summarize(group: &GroupState) -> RoomSummary {
    RoomSummary {
        room_id: group.group_id.clone(),
        epoch: group.epoch,
        members: group.members().len(),
    }
}

/// Build the deterministic command snapshot used by Tauri and the E2E harness.
#[must_use]
pub fn app_snapshot() -> AppSnapshot {
    AppSnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        friend: FriendView {
            alias: "Bob".to_owned(),
            friend_code: FIXTURE_BOB_FRIEND_CODE.to_owned(),
            safety_number: FIXTURE_BOB_SAFETY_NUMBER.to_owned(),
            verified: false,
        },
        devices: vec![
            DeviceView {
                device_id: "alice-laptop".to_owned(),
                leaf_index: 1,
                local: true,
                authorized: true,
            },
            DeviceView {
                device_id: "alice-phone".to_owned(),
                leaf_index: 2,
                local: false,
                authorized: true,
            },
        ],
        servers: vec![ServerView {
            name: "discrypt lab".to_owned(),
            role: "owner".to_owned(),
            channels: vec![
                ChannelView {
                    name: "#general".to_owned(),
                    kind: ChannelKind::Text,
                    retention_status: "7 day default; older messages lock, not vanish".to_owned(),
                },
                ChannelView {
                    name: "#ops".to_owned(),
                    kind: ChannelKind::Text,
                    retention_status: "shorten is retroactive; lengthen is future-only".to_owned(),
                },
                ChannelView {
                    name: "Voice Lobby".to_owned(),
                    kind: ChannelKind::Voice,
                    retention_status: "SFrame media; relays carry ciphertext only".to_owned(),
                },
            ],
        }],
        invite: InviteFlowView {
            expires: "Invite expires and can be revoked".to_owned(),
            max_use: "Max-use is enforced before MLS admission".to_owned(),
            password_gate: "Password rooms use OPAQUE/PAKE or an online authorized helper; no offline verifier".to_owned(),
            welcome_required: "Final admission still requires an authorized MLS Welcome/add".to_owned(),
        },
        retention: RetentionSettingsView {
            presets: vec![
                "1 hour".to_owned(),
                "24 hours".to_owned(),
                "7 days".to_owned(),
                "30 days".to_owned(),
                "90 days".to_owned(),
                "custom".to_owned(),
                "warned unlimited / never-lock".to_owned(),
            ],
            selected: "7 days".to_owned(),
            unlimited_warning: "Unlimited keeps local keys longer and weakens lock behavior; opt in explicitly".to_owned(),
            transition_copy: "Shortening re-locks older messages retroactively; lengthening applies only to future messages".to_owned(),
        },
        voice: VoiceRoomView {
            route: "STUN → peer relay overlay → TURN".to_owned(),
            relay_copy: "Relays see SFrame ciphertext only and active tamper is rejected".to_owned(),
            android_path: "Android uses encoded transforms when available, otherwise the native webrtc-rs contingency".to_owned(),
        },
        connectivity: ConnectivityView {
            fallback_chain: "STUN → relay-overlay → TURN; owner endpoints may override defaults".to_owned(),
            metadata_copy: "Content-private and metadata-minimizing, not metadata-anonymous".to_owned(),
            push_copy: "Android FCM wake is content-free and carries no room, sender, or message body".to_owned(),
        },
        security_copy: SecurityCopyView {
            metadata: "Passive infrastructure can see IPs and timing; discrypt does not claim anonymity".to_owned(),
            deletion: "Deleted on your online devices now; pending on offline devices until they reconnect".to_owned(),
            malicious_member: "Crypto-shred cannot erase screenshots, exports, modified clients, or plaintext already saved by a recipient".to_owned(),
        },
    }
}

/// Verify an out-of-band safety-number comparison.
#[must_use]
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    let snapshot = app_snapshot();
    let verified = request.friend_id == snapshot.friend.friend_code
        && request.provided == snapshot.friend.safety_number;
    SafetyVerificationResult {
        verified,
        message: if verified {
            "Safety number verified; MITM risk accepted by explicit user comparison".to_owned()
        } else {
            "Safety number mismatch; do not trust this device or DM".to_owned()
        },
    }
}

#[allow(dead_code)]
fn _invite_boundary(_: &Invite) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_snapshot_covers_required_ui_flows_and_copy() {
        let snapshot = app_snapshot();

        assert_eq!(snapshot.schema_version, SNAPSHOT_SCHEMA_VERSION);
        assert!(!snapshot.friend.verified);
        assert_eq!(snapshot, app_snapshot());
        assert_eq!(snapshot.devices.len(), 2);
        assert!(snapshot
            .servers
            .iter()
            .flat_map(|server| server.channels.iter())
            .any(|channel| channel.kind == ChannelKind::Voice));
        assert!(snapshot
            .invite
            .password_gate
            .contains("OPAQUE/PAKE or an online authorized helper"));
        assert!(snapshot
            .retention
            .presets
            .contains(&"warned unlimited / never-lock".to_owned()));
        assert!(snapshot
            .security_copy
            .deletion
            .contains("pending on offline devices until they reconnect"));
        assert!(snapshot
            .connectivity
            .metadata_copy
            .contains("not metadata-anonymous"));
    }

    #[test]
    fn safety_verification_requires_exact_match() {
        let snapshot = app_snapshot();
        let ok = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code.clone(),
            provided: snapshot.friend.safety_number.clone(),
        });
        let bad = verify_safety_number(SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code,
            provided: "9999".to_owned(),
        });

        assert!(ok.verified);
        assert!(!bad.verified);
    }
}
