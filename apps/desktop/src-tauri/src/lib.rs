//! Tauri command surface for the native discrypt shell.
use discrypt_core::{
    app_snapshot as core_app_snapshot, verify_safety_number as core_verify_safety_number,
    AppSnapshot, SafetyVerificationRequest, SafetyVerificationResult,
};
use serde::{Deserialize, Serialize};

/// Command result for local E2E/smoke execution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandHealth {
    /// Snapshot command returned all required UI flows.
    pub snapshot_ready: bool,
    /// Safety-number verification command accepts exact backend-owned matches.
    pub verification_ready: bool,
    /// Honest security copy is present for deletion and metadata claims.
    pub honest_copy_ready: bool,
}

/// Tauri command: return the initial app snapshot for the React shell.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn app_snapshot() -> AppSnapshot {
    core_app_snapshot()
}

/// Tauri command: verify a user-confirmed safety-number comparison.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn verify_safety_number(request: SafetyVerificationRequest) -> SafetyVerificationResult {
    core_verify_safety_number(request)
}

/// Tauri command: return the mandatory cooperative-deletion warning copy.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn deletion_warning() -> String {
    app_snapshot().security_copy.deletion
}

/// Tauri command: return the metadata-minimization caveat copy.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn metadata_warning() -> String {
    app_snapshot().security_copy.metadata
}

/// E2E command-health smoke used by CI and the multinode harness.
#[cfg_attr(feature = "tauri-runtime", tauri::command)]
#[must_use]
pub fn command_health() -> CommandHealth {
    let snapshot = app_snapshot();
    let verification = verify_safety_number(SafetyVerificationRequest {
        friend_id: snapshot.friend.friend_code.clone(),
        provided: snapshot.friend.safety_number.clone(),
    });
    CommandHealth {
        snapshot_ready: !snapshot.friend.verified
            && snapshot.schema_version == 1
            && snapshot.devices.len() >= 2
            && snapshot
                .servers
                .iter()
                .any(|server| !server.channels.is_empty()),
        verification_ready: verification.verified,
        honest_copy_ready: deletion_warning().contains("pending on offline devices")
            && metadata_warning().contains("does not claim anonymity"),
    }
}

/// Build and type-check the Tauri command handler registration.
#[cfg(feature = "tauri-runtime")]
#[must_use]
pub fn command_handler<R: tauri::Runtime>(
) -> impl Fn(tauri::ipc::Invoke<R>) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        app_snapshot,
        verify_safety_number,
        deletion_warning,
        metadata_warning,
        command_health
    ]
}

/// Run the native Tauri shell with the command surface registered for frontend IPC.
#[cfg(feature = "tauri-runtime")]
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(command_handler())
        .run(tauri::generate_context!())
        .expect("error while running discrypt Tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_surface_covers_snapshot_verification_and_honest_copy() {
        let health = command_health();
        assert!(health.snapshot_ready);
        assert!(health.verification_ready);
        assert!(health.honest_copy_ready);
    }
}
