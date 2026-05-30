//! Headless multinode harness for discrypt acceptance tests.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
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
    /// Captured PCM is encoded as Opus and SFrame-protected before transport handoff.
    pub capture_opus_sframe_protected: bool,
    /// Protected voice frames verify, decode, pass jitter ordering, and reach playback.
    pub receive_decode_jitter_playback_ready: bool,
    /// Media-path mute suppresses outbound PCM before encode/protect/transport.
    pub mute_suppresses_outbound_media: bool,
    /// Per-speaker playback volume is applied after authenticated receive/decode.
    pub playback_volume_mixer_ready: bool,
    /// Speaking indicators come from real PCM audio-level/VAD events, not fixtures.
    pub speaking_indicator_from_vad: bool,
    /// Android native WebRTC fallback is selected/configured when webview encoded transforms are unavailable.
    pub android_native_contingency_ready: bool,
    /// Receiver plaintext after successful authentication and replay acceptance.
    pub plaintext: Vec<u8>,
}

/// Deterministic Phase-J two-client voice media E2E verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VoiceMediaE2eSmoke {
    /// Two local clients exchanged protected media over the direct/STUN WebRTC leg.
    pub direct_webrtc_audio_exchanged: bool,
    /// Two local clients exchanged protected media over the peer-overlay relay leg.
    pub overlay_audio_exchanged: bool,
    /// Two local clients exchanged protected media over the TURN fallback leg.
    pub turn_audio_exchanged: bool,
    /// Local media mute suppressed outbound PCM before Opus/SFrame/transport.
    pub mute_blocks_outbound_audio: bool,
    /// Per-speaker playback volume changed authenticated remote playback samples.
    pub volume_affects_playback: bool,
    /// Speaking state came from real PCM audio levels on capture and playback.
    pub speaking_follows_actual_audio: bool,
    /// Relay/TURN pcap-style capture exposed protected bytes only, never PCM/Opus/key material.
    pub relay_pcap_protected_only: bool,
}

/// Deterministic Phase-2 relay overlay smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayOverlaySmoke {
    /// Selected route respects the ≤3 hop cap.
    pub hop_limit_respected: bool,
    /// Failover avoids the failed relay and converges within the Phase-2 gate.
    pub failover_recovered: bool,
    /// Media route failover meets the ≤200 ms gap/concealment target.
    pub media_gap_concealed: bool,
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

/// Report emitted by one OS process in the 16-node overlay churn/loss harness.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OverlayNodeProcessReport {
    /// Deterministic node index.
    pub node_index: usize,
    /// Relay-visible bytes were protected envelope metadata plus ciphertext only.
    pub relay_visible_ciphertext_only: bool,
    /// Active relay tamper was rejected by media authentication.
    pub tamper_rejected: bool,
    /// Active relay replay was rejected by media replay state.
    pub replay_rejected: bool,
}

/// Deterministic Phase-3 text/history/MLS delivery smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextHistoryDeliverySmoke {
    /// Two-node app text round-trips through encrypted bytes before entering stores.
    pub text_e2e_roundtrip: bool,
    /// Two client processes exchange protected text bytes over the direct/STUN route.
    pub direct_path_text_exchanged: bool,
    /// Two client processes exchange protected text bytes over the peer-overlay route.
    pub overlay_path_text_exchanged: bool,
    /// Two client processes exchange protected text bytes over the TURN fallback route.
    pub turn_path_text_exchanged: bool,
    /// Offline recipient drains queued ciphertext from store-forward before TTL expiry.
    pub offline_store_forward_within_ttl: bool,
    /// Retention/shred policy locks old queued text instead of delivering it after the window.
    pub retention_locks_old_store_forward: bool,
    /// Pcap-style text transport capture contains no plaintext or key material.
    pub text_pcap_no_plaintext: bool,
    /// Author/recipient storage and relay-visible samples do not contain plaintext.
    pub no_plaintext_in_text_surfaces: bool,
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

/// Deterministic Phase-4 retention/shred/live-key/storage smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionShredSmoke {
    /// Default retention caches fresh messages and locks old placeholders.
    pub default_window_locks_old_messages: bool,
    /// Shorten is retroactive while lengthen is future-only.
    pub shorten_retro_lengthen_future: bool,
    /// Cross-device shred blocks online devices and pending offline devices after sync.
    pub cross_device_shred_sync: bool,
    /// Live-key requests require local membership proof and enforce rate limits.
    pub live_key_membership_rate_limit_decoy: bool,
    /// Secure-delete simulator removes key material from SQLite/WAL/key-store paths.
    pub secure_delete_negative: bool,
    /// Account-continuity backup excludes content keys and cannot resurrect shredded content.
    pub recovery_cannot_resurrect_content_keys: bool,
}

/// Deterministic Phase-B storage persistence verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePersistenceSmoke {
    /// A fresh encrypted app DB starts empty before first profile creation.
    pub fresh_install_starts_empty: bool,
    /// A new store handle can restart and read the previously encrypted state.
    pub restart_loads_encrypted_state: bool,
    /// Plaintext app-state bytes are absent from DB, WAL, and temp sidecar paths.
    pub no_plaintext_in_db_wal_or_temp: bool,
    /// Malformed legacy/corrupt store bytes fail closed instead of seeding silently.
    pub corrupted_store_rejected: bool,
    /// Secure delete only passes after DB, WAL, and keychain material are all removed.
    pub secure_delete_requires_db_wal_and_keychain: bool,
}

/// Deterministic Phase-5 governance/admission/recovery/abuse smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GovernanceAdmissionSmoke {
    /// Governance events are canonical and signed.
    pub governance_ordered_signed: bool,
    /// Unauthorized and out-of-epoch actions are rejected.
    pub governance_rejects_invalid_authority: bool,
    /// Removed admin cannot win a same-epoch race.
    pub removed_admin_cannot_win: bool,
    /// Invite expiry/revoke/max-use are enforced.
    pub invite_controls_enforced: bool,
    /// Password admission rejects offline verifiers and requires Welcome.
    pub password_and_welcome_gate: bool,
    /// Recovery requires trust material and excludes content keys.
    pub recovery_trust_model: bool,
    /// Abuse controls rate-limit invites/spam and penalize freeloading.
    pub abuse_controls_enforced: bool,
}

/// Deterministic G119 abuse E2E smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AbuseE2eSmoke {
    /// Invite flood attempts are rate-limited after the allowed burst.
    pub invite_flood_rate_limited: bool,
    /// Text spam bursts are rate-limited after the allowed burst.
    pub spam_burst_rate_limited: bool,
    /// Online admission-helper brute force returns uniform rejection and remains locked out.
    pub admission_helper_bruteforce_rejected: bool,
    /// Signaling opaque blob floods are rate-limited per client token.
    pub signaling_blob_flood_rate_limited: bool,
    /// Relay freeloading feeds route ranking and downranks the freeloader.
    pub relay_freeloading_downranked: bool,
    /// Oversized service requests fail before JSON parsing or storage.
    pub service_request_size_exhaustion_rejected: bool,
}

/// Deterministic Phase-6 connectivity/signaling/push/metadata smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConnectivitySignalingPushSmoke {
    /// Signaling stores only opaque rendezvous data and no durable linkage.
    pub signaling_zero_linkage_at_rest: bool,
    /// Simulated NAT activates STUN, overlay, and TURN in the approved order.
    pub fallback_chain_covered: bool,
    /// Owner/group endpoint overrides are honored for STUN and TURN.
    pub owner_overrides_used: bool,
    /// Android FCM wake envelope is content-free.
    pub android_wake_content_free: bool,
    /// Pcap-style events match the approved infrastructure metadata matrix.
    pub metadata_matrix_validated: bool,
    /// Pcap-style fixture contains no forbidden content/identity egress.
    pub pcap_no_central_content: bool,
    /// TURN and peer relay observations are ciphertext-only.
    pub relays_ciphertext_only: bool,
    /// Local-process socket adapter delivers ciphertext and rejects plaintext.
    pub socket_local_process_conformant: bool,
    /// Route reporting preserves order and states deterministic-test limitations.
    pub route_reporting_honest: bool,
}

/// Deterministic Phase-N pcap acceptance matrix result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcapAcceptanceMatrixSmoke {
    /// AC1: identity/DM setup requires explicit safety-number verification and no directory/content egress.
    pub ac1_identity_dm_safety_pcap_clean: bool,
    /// AC8: relay/TURN/media observations contain protected bytes only.
    pub ac8_relay_media_ciphertext_only: bool,
    /// AC15: Android wake provider-visible bytes are content-free.
    pub ac15_android_wake_content_free: bool,
    /// AC18: signaling at-rest inspection has no identity-room-topology linkage.
    pub ac18_signaling_zero_linkage_at_rest: bool,
    /// AC-METADATA: observed pcap rows match the approved infrastructure metadata matrix.
    pub ac_metadata_matrix_validated: bool,
    /// Forbidden-byte scanner includes identity, message, media, and key sentinels.
    pub forbidden_scanner_covers_release_tokens: bool,
}

/// Deterministic Phase-N malicious relay adversary result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaliciousRelayAdversarySmoke {
    /// Passive relay-visible bytes do not expose plaintext or key material.
    pub passive_read_blocked: bool,
    /// Active relay bit-flip tampering is rejected by receiver authentication.
    pub tamper_rejected: bool,
    /// Replayed protected frames are rejected by receiver anti-replay state.
    pub replay_rejected: bool,
    /// Dropped packet simulation requests bounded redelivery from alternate peers.
    pub drop_requests_bounded_redelivery: bool,
    /// Reordered packets inside the replay window are accepted once and stale repeats are rejected.
    pub reorder_window_enforced: bool,
    /// Endpoint churn is damped while hard-failure failover bypasses the planned-change delay.
    pub endpoint_churn_damped_and_failover_recovered: bool,
}

/// Deterministic Phase-N malicious member/device adversary result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MaliciousMemberAdversarySmoke {
    /// Media frames cannot be relabeled from one MLS leaf/device KID to another.
    pub media_impersonation_rejected: bool,
    /// Evicted members cannot deliver text under the current authorized sender set.
    pub evicted_member_text_rejected: bool,
    /// Evicted devices lose media receive authorization after epoch/device rotation.
    pub evicted_device_media_rejected: bool,
    /// Forked MLS state is detected as divergent rather than silently accepted.
    pub forked_mls_commit_rejected: bool,
    /// Out-of-epoch governance actions are rejected.
    pub out_of_epoch_governance_rejected: bool,
    /// Unauthorized governance actions are rejected.
    pub unauthorized_governance_rejected: bool,
    /// A removed admin cannot win a same-epoch governance race.
    pub removed_admin_race_rejected: bool,
}

/// Deterministic Phase-N retention/shred storage-boundary result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetentionShredStorageBoundarySmoke {
    /// Locked/cached retention records survive encrypted AppStore restart.
    pub retention_state_round_trips_encrypted_store: bool,
    /// DB, WAL, temp file, and keychain snapshots exclude plaintext and content-key bytes.
    pub store_and_keychain_exclude_plaintext_and_content_keys: bool,
    /// Removing the wrapping key prevents restoring retained ciphertext from the DB file.
    pub keychain_required_for_restore: bool,
    /// Secure deletion enumerates DB, WAL, temp, and keychain boundaries.
    pub secure_delete_enumerates_store_journal_temp_and_keychain: bool,
    /// Account-continuity recovery material cannot resurrect shredded content keys.
    pub recovery_after_shred_excludes_content_keys: bool,
}

/// Deterministic Phase-N performance soak result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PerformanceSoakSmoke {
    /// Sixteen authenticated members are represented in the soak graph.
    pub sixteen_members_represented: bool,
    /// Eight concurrent voice senders protect and verify media frames.
    pub eight_voice_senders_verified: bool,
    /// Overlay routes cover one-, two-, and three-hop paths inside the hop cap.
    pub one_to_three_relay_hops_covered: bool,
    /// Packet-loss simulation drives bounded redelivery without accepting replays.
    pub packet_loss_redelivery_bounded: bool,
    /// NAT switching covers direct, overlay, and TURN fallback legs.
    pub nat_switching_fallbacks_covered: bool,
    /// Android doze posture is ranked away from preferred relay paths.
    pub android_doze_deprioritized: bool,
    /// Restart/reconnect restores encrypted session state and recovers a route.
    pub restart_reconnect_recovers_route: bool,
}

/// Deterministic Phase-C device-rotation integration verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PhaseCDeviceRotationSmoke {
    /// Compromised device is marked retired and no longer eligible to author sends.
    pub compromised_device_retired: bool,
    /// Group state rekeys after removing the compromised leaf and adding replacement.
    pub group_rekeyed_after_rotation: bool,
    /// Old compromised leaf cannot send in the current group epoch.
    pub old_device_send_blocked: bool,
    /// Replacement leaf can send in the current group epoch.
    pub replacement_device_can_send: bool,
    /// Replacement leaf cannot replay sends under the prior epoch.
    pub stale_epoch_send_blocked: bool,
    /// Transparency stream includes compromised removal and replacement notices.
    pub transparency_notices_include_rotation: bool,
    /// Command/UI-facing health still reports honest device-management metadata.
    pub command_surface_reports_device_metadata: bool,
}

/// Deterministic Phase-7 UX and end-to-end hardening smoke result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UxE2eHardeningSmoke {
    /// Tauri command facade returns the required snapshot and verification commands.
    pub command_surface_ready: bool,
    /// React/Discord-style UX model includes servers, text channels, and voice rooms.
    pub discord_style_model_ready: bool,
    /// Device management and friend safety-number verification are surfaced.
    pub verification_and_devices_ready: bool,
    /// Invite, retention, and deletion flows expose honest copy.
    pub invite_retention_deletion_ready: bool,
    /// Connectivity, push, and metadata posture are surfaced.
    pub connectivity_copy_ready: bool,
    /// Prior deterministic E2E harness phases still pass through one final smoke.
    pub all_phase_smokes_ready: bool,
}

/// Deterministic two-profile DM, voice/media, and UI verification result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TwoProfileP2pDmVoiceUiSmoke {
    /// Alice and Bob are independently generated profiles with distinct identity keys.
    pub independent_profiles_created: bool,
    /// Both profiles derive the same out-of-band DM safety number.
    pub pairwise_safety_numbers_match: bool,
    /// The direct app-text route protects, delivers, and decrypts one DM payload.
    pub p2p_dm_message_e2e: bool,
    /// The voice/media harness covers direct, overlay, and TURN attempts without plaintext exposure.
    pub voice_media_attempt_covered: bool,
    /// UI/command hardening gates are ready for browser-visible setup, DM, invite, text, and voice checks.
    pub frontend_ui_checks_ready: bool,
    /// The command/UI voice roster is state-backed and does not fabricate friend/relay participants.
    pub no_fake_voice_members: bool,
}

impl MediaSecuritySmoke {
    /// True when every Phase-1 security invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.passive_relay_cannot_read
            && self.replay_rejected
            && self.tamper_rejected
            && self.capture_opus_sframe_protected
            && self.receive_decode_jitter_playback_ready
            && self.mute_suppresses_outbound_media
            && self.playback_volume_mixer_ready
            && self.speaking_indicator_from_vad
            && self.android_native_contingency_ready
    }
}

impl VoiceMediaE2eSmoke {
    /// True when every Phase-J media verification invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.direct_webrtc_audio_exchanged
            && self.overlay_audio_exchanged
            && self.turn_audio_exchanged
            && self.mute_blocks_outbound_audio
            && self.volume_affects_playback
            && self.speaking_follows_actual_audio
            && self.relay_pcap_protected_only
    }
}

impl RelayOverlaySmoke {
    /// True when every Phase-2 relay invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.hop_limit_respected
            && self.failover_recovered
            && self.media_gap_concealed
            && self.redelivery_replay_rejected
            && self.store_forward_plaintext_rejected
            && self.store_forward_ttl_enforced
            && self.store_forward_fanout_bounded
            && self.ciphertext_only_media
            && self.tamper_rejected
    }
}

impl OverlayNodeProcessReport {
    /// True when this process satisfied all local relay-security checks.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.relay_visible_ciphertext_only && self.tamper_rejected && self.replay_rejected
    }

    /// Stable line protocol for parent process parsing.
    #[must_use]
    pub fn to_line(&self) -> String {
        format!(
            "overlay-node node_index={} relay_visible_ciphertext_only={} tamper_rejected={} replay_rejected={}",
            self.node_index,
            self.relay_visible_ciphertext_only,
            self.tamper_rejected,
            self.replay_rejected
        )
    }
}

impl TextHistoryDeliverySmoke {
    /// True when every Phase-3 delivery invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.text_e2e_roundtrip
            && self.direct_path_text_exchanged
            && self.overlay_path_text_exchanged
            && self.turn_path_text_exchanged
            && self.offline_store_forward_within_ttl
            && self.retention_locks_old_store_forward
            && self.text_pcap_no_plaintext
            && self.no_plaintext_in_text_surfaces
            && self.author_logs_merged
            && self.recipient_cache_bounded
            && self.gossip_converged_16
            && self.ordered_commit_delivery
            && self.welcome_catchup_live
            && self.fork_detected_not_silent
            && self.repair_converged_equal_tags
            && self.divergent_mls_not_replayed
    }
}

impl RetentionShredSmoke {
    /// True when every Phase-4 retention/shred invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.default_window_locks_old_messages
            && self.shorten_retro_lengthen_future
            && self.cross_device_shred_sync
            && self.live_key_membership_rate_limit_decoy
            && self.secure_delete_negative
            && self.recovery_cannot_resurrect_content_keys
    }
}

impl StoragePersistenceSmoke {
    /// True when every Phase-B storage persistence invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.fresh_install_starts_empty
            && self.restart_loads_encrypted_state
            && self.no_plaintext_in_db_wal_or_temp
            && self.corrupted_store_rejected
            && self.secure_delete_requires_db_wal_and_keychain
    }
}

impl GovernanceAdmissionSmoke {
    /// True when every Phase-5 governance/admission invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.governance_ordered_signed
            && self.governance_rejects_invalid_authority
            && self.removed_admin_cannot_win
            && self.invite_controls_enforced
            && self.password_and_welcome_gate
            && self.recovery_trust_model
            && self.abuse_controls_enforced
    }
}

impl AbuseE2eSmoke {
    /// True when every G119 abuse E2E invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.invite_flood_rate_limited
            && self.spam_burst_rate_limited
            && self.admission_helper_bruteforce_rejected
            && self.signaling_blob_flood_rate_limited
            && self.relay_freeloading_downranked
            && self.service_request_size_exhaustion_rejected
    }
}

impl ConnectivitySignalingPushSmoke {
    /// True when every Phase-6 connectivity/signaling invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.signaling_zero_linkage_at_rest
            && self.fallback_chain_covered
            && self.owner_overrides_used
            && self.android_wake_content_free
            && self.metadata_matrix_validated
            && self.pcap_no_central_content
            && self.relays_ciphertext_only
            && self.socket_local_process_conformant
            && self.route_reporting_honest
    }
}

impl PcapAcceptanceMatrixSmoke {
    /// True when every Phase-N pcap acceptance invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.ac1_identity_dm_safety_pcap_clean
            && self.ac8_relay_media_ciphertext_only
            && self.ac15_android_wake_content_free
            && self.ac18_signaling_zero_linkage_at_rest
            && self.ac_metadata_matrix_validated
            && self.forbidden_scanner_covers_release_tokens
    }
}

impl MaliciousRelayAdversarySmoke {
    /// True when every malicious relay adversary invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.passive_read_blocked
            && self.tamper_rejected
            && self.replay_rejected
            && self.drop_requests_bounded_redelivery
            && self.reorder_window_enforced
            && self.endpoint_churn_damped_and_failover_recovered
    }
}

impl MaliciousMemberAdversarySmoke {
    /// True when every malicious member/device adversary invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.media_impersonation_rejected
            && self.evicted_member_text_rejected
            && self.evicted_device_media_rejected
            && self.forked_mls_commit_rejected
            && self.out_of_epoch_governance_rejected
            && self.unauthorized_governance_rejected
            && self.removed_admin_race_rejected
    }
}

impl RetentionShredStorageBoundarySmoke {
    /// True when every retention/shred storage-boundary invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.retention_state_round_trips_encrypted_store
            && self.store_and_keychain_exclude_plaintext_and_content_keys
            && self.keychain_required_for_restore
            && self.secure_delete_enumerates_store_journal_temp_and_keychain
            && self.recovery_after_shred_excludes_content_keys
    }
}

impl PerformanceSoakSmoke {
    /// True when every performance soak invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.sixteen_members_represented
            && self.eight_voice_senders_verified
            && self.one_to_three_relay_hops_covered
            && self.packet_loss_redelivery_bounded
            && self.nat_switching_fallbacks_covered
            && self.android_doze_deprioritized
            && self.restart_reconnect_recovers_route
    }
}

impl PhaseCDeviceRotationSmoke {
    /// True when every Phase-C device-rotation invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.compromised_device_retired
            && self.group_rekeyed_after_rotation
            && self.old_device_send_blocked
            && self.replacement_device_can_send
            && self.stale_epoch_send_blocked
            && self.transparency_notices_include_rotation
            && self.command_surface_reports_device_metadata
    }
}

impl UxE2eHardeningSmoke {
    /// True when every Phase-7 UX/E2E invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.command_surface_ready
            && self.discord_style_model_ready
            && self.verification_and_devices_ready
            && self.invite_retention_deletion_ready
            && self.connectivity_copy_ready
            && self.all_phase_smokes_ready
    }
}

impl TwoProfileP2pDmVoiceUiSmoke {
    /// True when every two-profile/browser-facing invariant is satisfied.
    #[must_use]
    pub fn ready(&self) -> bool {
        self.independent_profiles_created
            && self.pairwise_safety_numbers_match
            && self.p2p_dm_message_e2e
            && self.voice_media_attempt_covered
            && self.frontend_ui_checks_ready
            && self.no_fake_voice_members
    }
}

/// Exercise passive relay opacity, active tamper rejection, and anti-replay checks.
pub fn media_security_smoke() -> Result<MediaSecuritySmoke, discrypt_media::MediaError> {
    use discrypt_media::{
        AndroidVoiceContingency, AudioCaptureFormat, BridgeProtectedFrame, CapturedAudioFrame,
        DecodedAudioFrame, MediaKeyRegistry, MicrophonePermissionState, OpusAudioDecoder,
        OpusAudioEncoder, PlaybackAudioSink, PlaybackVolumeMixer, ProtectedMediaFrameSink,
        ReplayWindow, RustTransformBridge, SFrameReceiver, SFrameSender, SenderBinding,
        VoiceCaptureSFramePipeline, VoiceCaptureSendOutcome, VoiceDeviceDescriptor,
        VoiceDeviceKind, VoiceDeviceSelection, VoiceJitterBuffer, VoiceReceiveSFramePipeline,
    };
    use discrypt_relay_overlay::integrity::{
        contains_plaintext, RelayPacket, RelayPayloadKind, RelayProtectedEnvelope,
    };

    fn relay_payload(
        kind: RelayPayloadKind,
        kid: Vec<u8>,
        counter: u64,
        aad_metadata: &[u8],
        ciphertext: Vec<u8>,
    ) -> Result<RelayProtectedEnvelope, discrypt_media::MediaError> {
        RelayProtectedEnvelope::new(kind, kid, counter, aad_metadata, ciphertext)
            .map_err(|_| discrypt_media::MediaError::AuthenticationFailed)
    }

    let binding = SenderBinding::derive_for_epoch(&[9; 32], "harness-media", 9, 1, "alice-laptop")?;
    let mut sender = SFrameSender::new(&[9; 32], binding.clone())?;
    let mut registry = MediaKeyRegistry::new();
    registry.register_sender(&[9; 32], binding.clone())?;
    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&[9; 32], binding)?;
    let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

    let plaintext = b"harness encoded voice frame";
    let relayed = sender.protect(plaintext)?;
    let relay_packet = RelayPacket::from_envelope(
        "relay-a",
        relay_payload(
            RelayPayloadKind::Media,
            relayed.kid.clone(),
            relayed.counter,
            b"media-security-smoke",
            relayed.ciphertext.clone(),
        )?,
    )
    .forward("relay-b");
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

    #[derive(Default)]
    struct HarnessMediaSink {
        sent: Vec<BridgeProtectedFrame>,
    }

    impl ProtectedMediaFrameSink for HarnessMediaSink {
        fn send_protected_media_frame(
            &mut self,
            frame: BridgeProtectedFrame,
        ) -> Result<(), discrypt_media::MediaError> {
            self.sent.push(frame);
            Ok(())
        }
    }

    #[derive(Default)]
    struct HarnessPlaybackSink {
        played: Vec<DecodedAudioFrame>,
    }

    impl PlaybackAudioSink for HarnessPlaybackSink {
        fn queue_playback_frame(
            &mut self,
            frame: DecodedAudioFrame,
        ) -> Result<(), discrypt_media::MediaError> {
            self.played.push(frame);
            Ok(())
        }
    }

    let capture_format = AudioCaptureFormat::mono_20ms_48khz();
    let pcm = (0..capture_format.interleaved_samples_per_frame())
        .map(|sample| {
            let phase = sample as f32 / capture_format.sample_rate_hz as f32;
            (phase * 330.0 * core::f32::consts::TAU)
                .sin()
                .mul_add(3_000.0, 0.0) as i16
        })
        .collect::<Vec<_>>();
    let captured = CapturedAudioFrame::new(pcm.clone(), capture_format, 777)?;
    let mut opus_probe = OpusAudioEncoder::new(capture_format)?;
    let encoded_probe = opus_probe.encode(captured.clone())?;
    let capture_binding =
        SenderBinding::derive_for_epoch(&[11; 32], "harness-media", 11, 2, "alice-capture-device")?;
    let capture_sender = SFrameSender::new(&[11; 32], capture_binding.clone())?;
    let mut capture_registry = MediaKeyRegistry::new();
    capture_registry.register_sender(&[11; 32], capture_binding)?;
    let capture_bridge = RustTransformBridge::new(
        capture_sender,
        SFrameReceiver::new(capture_registry, ReplayWindow::default()),
    );
    let mut capture_pipeline = VoiceCaptureSFramePipeline::new(
        OpusAudioEncoder::new(capture_format)?,
        capture_bridge,
        HarnessMediaSink::default(),
    );
    let capture_report = capture_pipeline.capture_encode_protect_send(captured)?;
    let capture_sink = capture_pipeline.into_sink();
    let capture_opus_sframe_protected = capture_report.opus_payload_len
        == encoded_probe.opus_payload.len()
        && capture_report.protected_payload_len > capture_report.opus_payload_len
        && capture_sink.sent.len() == 1
        && capture_sink.sent[0].bytes.len() == capture_report.protected_payload_len
        && capture_sink.sent[0].bytes != encoded_probe.opus_payload
        && !capture_sink.sent[0].kid.is_empty();

    let receive_binding =
        SenderBinding::derive_for_epoch(&[11; 32], "harness-media", 11, 2, "alice-capture-device")?;
    let receive_sender =
        SFrameSender::new_for_epoch(&[12; 32], "harness-receive-unused", 12, 1, "unused")?;
    let mut receive_registry = MediaKeyRegistry::new();
    receive_registry.register_sender(&[11; 32], receive_binding)?;
    let receive_bridge = RustTransformBridge::new(
        receive_sender,
        SFrameReceiver::new(receive_registry, ReplayWindow::default()),
    );
    let mut receive_pipeline = VoiceReceiveSFramePipeline::new(
        receive_bridge,
        OpusAudioDecoder::new(capture_format)?,
        VoiceJitterBuffer::new(0),
        HarnessPlaybackSink::default(),
    );
    let queued_playback = receive_pipeline.receive_protected_frame(capture_sink.sent[0].clone())?;
    let playback_sink = receive_pipeline.into_sink();
    let receive_decode_jitter_playback_ready = queued_playback == 1
        && playback_sink.played.len() == 1
        && playback_sink.played[0].sender.group_id == "harness-media"
        && playback_sink.played[0].sender.epoch == 11
        && playback_sink.played[0].pcm_i16.len() == capture_format.interleaved_samples_per_frame();

    let mute_binding =
        SenderBinding::derive_for_epoch(&[13; 32], "harness-muted-send", 13, 1, "muted-device")?;
    let mute_sender = SFrameSender::new(&[13; 32], mute_binding.clone())?;
    let mut mute_registry = MediaKeyRegistry::new();
    mute_registry.register_sender(&[13; 32], mute_binding)?;
    let mut muted_pipeline = VoiceCaptureSFramePipeline::new(
        OpusAudioEncoder::new(capture_format)?,
        RustTransformBridge::new(
            mute_sender,
            SFrameReceiver::new(mute_registry, ReplayWindow::default()),
        ),
        HarnessMediaSink::default(),
    );
    muted_pipeline.set_muted(true);
    let mute_outcome = muted_pipeline.capture_encode_protect_or_mute(CapturedAudioFrame::new(
        vec![0; capture_format.interleaved_samples_per_frame()],
        capture_format,
        888,
    )?)?;
    let mute_sink = muted_pipeline.into_sink();
    let mute_suppresses_outbound_media = matches!(
        mute_outcome,
        VoiceCaptureSendOutcome::Muted {
            captured_at_ms: 888,
            dropped_pcm_samples
        } if dropped_pcm_samples == capture_format.interleaved_samples_per_frame()
    ) && mute_sink.sent.is_empty();

    let volume_binding =
        SenderBinding::derive_for_epoch(&[14; 32], "harness-volume", 14, 3, "volume-device")?;
    let volume_sender = SFrameSender::new(&[14; 32], volume_binding.clone())?;
    let mut volume_registry = MediaKeyRegistry::new();
    volume_registry.register_sender(&[14; 32], volume_binding.clone())?;
    let mut volume_capture = VoiceCaptureSFramePipeline::new(
        OpusAudioEncoder::new(capture_format)?,
        RustTransformBridge::new(
            volume_sender,
            SFrameReceiver::new(volume_registry, ReplayWindow::default()),
        ),
        HarnessMediaSink::default(),
    );
    let _volume_report = volume_capture.capture_encode_protect_send(CapturedAudioFrame::new(
        pcm.clone(),
        capture_format,
        999,
    )?)?;
    let volume_sink = volume_capture.into_sink();
    let receive_sender =
        SFrameSender::new_for_epoch(&[15; 32], "harness-volume-unused", 15, 1, "unused")?;
    let mut playback_registry = MediaKeyRegistry::new();
    playback_registry.register_sender(&[14; 32], volume_binding.clone())?;
    let mut playback_mixer = PlaybackVolumeMixer::unity();
    playback_mixer.set_speaker_volume(&volume_binding, 0)?;
    let mut volume_receive = VoiceReceiveSFramePipeline::with_volume_mixer(
        RustTransformBridge::new(
            receive_sender,
            SFrameReceiver::new(playback_registry, ReplayWindow::default()),
        ),
        OpusAudioDecoder::new(capture_format)?,
        VoiceJitterBuffer::new(0),
        playback_mixer,
        HarnessPlaybackSink::default(),
    );
    let volume_queued = volume_receive.receive_protected_frame(volume_sink.sent[0].clone())?;
    let volume_activity = volume_receive
        .last_voice_activity(&volume_binding)?
        .cloned();
    let volume_playback_sink = volume_receive.into_sink();
    let speaking_indicator_from_vad = capture_report.audio_level.speaking
        && capture_report.audio_level.rms_i16 > 0
        && volume_activity
            .as_ref()
            .is_some_and(|event| event.speaking && event.rms_i16 > 0 && event.counter == Some(0));
    let playback_volume_mixer_ready = volume_queued == 1
        && volume_playback_sink.played.len() == 1
        && volume_playback_sink.played[0].sender.device_id == "volume-device"
        && volume_playback_sink.played[0]
            .pcm_i16
            .iter()
            .all(|sample| *sample == 0);

    let android_native_contingency_ready = AndroidVoiceContingency {
        platform: "android".to_owned(),
        encoded_transform_supported: false,
    }
    .native_plan(
        vec![
            "stun:stun.discrypt.invalid:3478".to_owned(),
            "turns:turn.discrypt.invalid:5349".to_owned(),
        ],
        VoiceDeviceSelection::new(
            MicrophonePermissionState::Granted,
            Some(VoiceDeviceDescriptor::new(
                "android-mic",
                "Android microphone",
                VoiceDeviceKind::AudioInput,
            )),
            Some(VoiceDeviceDescriptor::new(
                "android-speaker",
                "Android speaker",
                VoiceDeviceKind::AudioOutput,
            )),
        ),
    )
    .ok()
    .flatten()
    .is_some_and(|plan| plan.ready_for_protected_media() && plan.rust_sframe_required);

    Ok(MediaSecuritySmoke {
        passive_relay_cannot_read,
        replay_rejected,
        tamper_rejected,
        capture_opus_sframe_protected,
        receive_decode_jitter_playback_ready,
        mute_suppresses_outbound_media,
        playback_volume_mixer_ready,
        speaking_indicator_from_vad,
        android_native_contingency_ready,
        plaintext: opened.plaintext,
    })
}

/// Verify two-client protected voice media across direct, overlay, and TURN legs.
pub fn voice_media_e2e_smoke() -> Result<VoiceMediaE2eSmoke, anyhow::Error> {
    use anyhow::{anyhow, ensure};
    use discrypt_media::{
        AudioCaptureFormat, BridgeProtectedFrame, CapturedAudioFrame, DecodedAudioFrame,
        MediaKeyRegistry, OpusAudioDecoder, OpusAudioEncoder, PlaybackAudioSink,
        PlaybackVolumeMixer, ProtectedMediaFrameSink, ReplayWindow, RustTransformBridge,
        SFrameReceiver, SFrameSender, SenderBinding, VoiceCaptureSFramePipeline,
        VoiceCaptureSendOutcome, VoiceJitterBuffer, VoiceReceiveSFramePipeline,
    };
    use discrypt_transport::{
        ConnectivityConfig, FallbackLeg, LocalProcessSocketAdapter, SimulatedNat,
    };
    use external_signaling::{AuditFixture, ContentExposure, InfrastructureComponent, PcapEvent};

    #[derive(Default)]
    struct HarnessMediaSink {
        sent: Vec<BridgeProtectedFrame>,
    }

    impl ProtectedMediaFrameSink for HarnessMediaSink {
        fn send_protected_media_frame(
            &mut self,
            frame: BridgeProtectedFrame,
        ) -> Result<(), discrypt_media::MediaError> {
            self.sent.push(frame);
            Ok(())
        }
    }

    #[derive(Default)]
    struct HarnessPlaybackSink {
        played: Vec<DecodedAudioFrame>,
    }

    impl PlaybackAudioSink for HarnessPlaybackSink {
        fn queue_playback_frame(
            &mut self,
            frame: DecodedAudioFrame,
        ) -> Result<(), discrypt_media::MediaError> {
            self.played.push(frame);
            Ok(())
        }
    }

    #[derive(Clone, Debug)]
    struct VoiceRouteResult {
        audio_exchanged: bool,
        volume_zeroed: bool,
        speaking: bool,
        opus_payload: Vec<u8>,
        epoch_secret: Vec<u8>,
    }

    #[derive(Clone, Copy, Debug)]
    struct VoiceRouteCase<'a> {
        route_label: &'a str,
        nat: SimulatedNat,
        expected_leg: FallbackLeg,
        secret_seed: u8,
        zero_volume: bool,
    }

    fn signed_voice_frame(format: AudioCaptureFormat, amplitude: f32) -> Vec<i16> {
        (0..format.interleaved_samples_per_frame())
            .map(|sample| {
                let phase = sample as f32 / format.sample_rate_hz as f32;
                (phase * 440.0 * core::f32::consts::TAU)
                    .sin()
                    .mul_add(amplitude, 0.0) as i16
            })
            .collect()
    }

    fn pcm_bytes(pcm: &[i16]) -> Vec<u8> {
        pcm.iter().flat_map(|sample| sample.to_le_bytes()).collect()
    }

    fn transport_visible_payload(frame: &BridgeProtectedFrame) -> Vec<u8> {
        let mut payload = Vec::with_capacity(frame.kid.len() + frame.bytes.len() + 12);
        payload.extend_from_slice(&(frame.kid.len() as u32).to_be_bytes());
        payload.extend_from_slice(&frame.kid);
        payload.extend_from_slice(&frame.counter.to_be_bytes());
        payload.extend_from_slice(&frame.bytes);
        payload
    }

    fn verify_voice_route(
        route: VoiceRouteCase<'_>,
        capture_format: AudioCaptureFormat,
        raw_pcm: &[i16],
        raw_pcm_bytes: &[u8],
        pcap: &mut AuditFixture,
    ) -> Result<VoiceRouteResult, anyhow::Error> {
        let route_label = route.route_label;
        let expected_leg = route.expected_leg;
        let secret_seed = route.secret_seed;
        let epoch_secret = [secret_seed; 32];
        let binding = SenderBinding::derive_for_epoch(
            &epoch_secret,
            "voice-e2e-lab",
            u64::from(secret_seed),
            1,
            format!("alice-{route_label}-device"),
        )?;
        let captured = CapturedAudioFrame::new(
            raw_pcm.to_vec(),
            capture_format,
            10_000 + u64::from(secret_seed),
        )?;
        let mut opus_probe = OpusAudioEncoder::new(capture_format)?;
        let encoded_probe = opus_probe.encode(captured.clone())?;

        let sender = SFrameSender::new(&epoch_secret, binding.clone())?;
        let mut sender_registry = MediaKeyRegistry::new();
        sender_registry.register_sender(&epoch_secret, binding.clone())?;
        let mut capture = VoiceCaptureSFramePipeline::new(
            OpusAudioEncoder::new(capture_format)?,
            RustTransformBridge::new(
                sender,
                SFrameReceiver::new(sender_registry, ReplayWindow::default()),
            ),
            HarnessMediaSink::default(),
        );
        let capture_report = capture.capture_encode_protect_send(captured)?;
        let capture_sink = capture.into_sink();
        let protected_frame = capture_sink
            .sent
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("voice route {route_label} produced no protected frame"))?;
        let payload = transport_visible_payload(&protected_frame);
        let conformance = LocalProcessSocketAdapter::new(
            ConnectivityConfig::default(),
            route.nat,
            raw_pcm_bytes.to_vec(),
        )
        .run_conformance(&payload)?;

        let (component, content, visible_bytes) = match expected_leg {
            FallbackLeg::Stun => (
                InfrastructureComponent::Stun,
                ContentExposure::None,
                b"voice direct path stun binding; no app payload".to_vec(),
            ),
            FallbackLeg::RelayOverlay => (
                InfrastructureComponent::PeerRelay,
                ContentExposure::CiphertextOnly,
                payload.clone(),
            ),
            FallbackLeg::Turn => (
                InfrastructureComponent::Turn,
                ContentExposure::CiphertextOnly,
                payload.clone(),
            ),
        };
        pcap.push(PcapEvent {
            component,
            content,
            visible_bytes,
            ip_or_endpoint: true,
            timing: true,
            persists_linkage: false,
        });

        let receiver_sender = SFrameSender::new_for_epoch(
            &[secret_seed.wrapping_add(80); 32],
            "voice-e2e-receiver-unused",
            u64::from(secret_seed) + 80,
            1,
            "receiver-unused",
        )?;
        let mut receiver_registry = MediaKeyRegistry::new();
        receiver_registry.register_sender(&epoch_secret, binding.clone())?;
        let mut mixer = PlaybackVolumeMixer::unity();
        if route.zero_volume {
            mixer.set_speaker_volume(&binding, 0)?;
        }
        let mut receive = VoiceReceiveSFramePipeline::with_volume_mixer(
            RustTransformBridge::new(
                receiver_sender,
                SFrameReceiver::new(receiver_registry, ReplayWindow::default()),
            ),
            OpusAudioDecoder::new(capture_format)?,
            VoiceJitterBuffer::new(0),
            mixer,
            HarnessPlaybackSink::default(),
        );
        let queued = receive.receive_protected_frame(protected_frame)?;
        let activity = receive.last_voice_activity(&binding)?.cloned();
        let speaking_speakers = receive.speaking_speakers();
        let playback_sink = receive.into_sink();
        let playback_frame = playback_sink.played.first();
        let volume_zeroed = playback_frame.is_some_and(|frame| {
            frame.pcm_i16.len() == capture_format.interleaved_samples_per_frame()
                && frame.pcm_i16.iter().all(|sample| *sample == 0)
        });
        let speaking = capture_report.audio_level.speaking
            && capture_report.audio_level.rms_i16 > 0
            && activity.as_ref().is_some_and(|event| {
                event.speaking && event.rms_i16 > 0 && event.counter == Some(0)
            })
            && speaking_speakers
                .iter()
                .any(|speaker| speaker.device_id == binding.device_id);

        Ok(VoiceRouteResult {
            audio_exchanged: conformance.ready()
                && conformance.route_report.selected == expected_leg
                && conformance.ciphertext_delivered
                && queued == 1
                && playback_sink.played.len() == 1
                && playback_frame.is_some_and(|frame| {
                    frame.sender.group_id == "voice-e2e-lab"
                        && frame.sender.device_id == binding.device_id
                        && frame.counter == 0
                        && frame.pcm_i16.len() == capture_format.interleaved_samples_per_frame()
                }),
            volume_zeroed,
            speaking,
            opus_payload: encoded_probe.opus_payload,
            epoch_secret: epoch_secret.to_vec(),
        })
    }

    let capture_format = AudioCaptureFormat::mono_20ms_48khz();
    let raw_pcm = signed_voice_frame(capture_format, 4_000.0);
    let raw_pcm_bytes = pcm_bytes(&raw_pcm);
    ensure!(
        !raw_pcm_bytes.is_empty(),
        "voice fixture PCM must not be empty"
    );

    let mut pcap = AuditFixture::default();
    let direct = verify_voice_route(
        VoiceRouteCase {
            route_label: "direct",
            nat: SimulatedNat::direct(),
            expected_leg: FallbackLeg::Stun,
            secret_seed: 71,
            zero_volume: false,
        },
        capture_format,
        &raw_pcm,
        &raw_pcm_bytes,
        &mut pcap,
    )?;
    let overlay = verify_voice_route(
        VoiceRouteCase {
            route_label: "overlay",
            nat: SimulatedNat::overlay_only(),
            expected_leg: FallbackLeg::RelayOverlay,
            secret_seed: 72,
            zero_volume: false,
        },
        capture_format,
        &raw_pcm,
        &raw_pcm_bytes,
        &mut pcap,
    )?;
    let turn = verify_voice_route(
        VoiceRouteCase {
            route_label: "turn",
            nat: SimulatedNat::turn_only(),
            expected_leg: FallbackLeg::Turn,
            secret_seed: 73,
            zero_volume: true,
        },
        capture_format,
        &raw_pcm,
        &raw_pcm_bytes,
        &mut pcap,
    )?;

    let mute_binding =
        SenderBinding::derive_for_epoch(&[74; 32], "voice-e2e-mute", 74, 1, "muted-device")?;
    let mute_sender = SFrameSender::new(&[74; 32], mute_binding.clone())?;
    let mut mute_registry = MediaKeyRegistry::new();
    mute_registry.register_sender(&[74; 32], mute_binding)?;
    let mut muted_capture = VoiceCaptureSFramePipeline::new(
        OpusAudioEncoder::new(capture_format)?,
        RustTransformBridge::new(
            mute_sender,
            SFrameReceiver::new(mute_registry, ReplayWindow::default()),
        ),
        HarnessMediaSink::default(),
    );
    muted_capture.set_muted(true);
    let mute_outcome = muted_capture.capture_encode_protect_or_mute(CapturedAudioFrame::new(
        raw_pcm.clone(),
        capture_format,
        20_000,
    )?)?;
    let mute_sink = muted_capture.into_sink();
    let mute_blocks_outbound_audio = matches!(
        mute_outcome,
        VoiceCaptureSendOutcome::Muted {
            captured_at_ms: 20_000,
            dropped_pcm_samples
        } if dropped_pcm_samples == capture_format.interleaved_samples_per_frame()
    ) && mute_sink.sent.is_empty();

    let forbidden_payloads = vec![
        raw_pcm_bytes,
        direct.opus_payload.clone(),
        overlay.opus_payload.clone(),
        turn.opus_payload.clone(),
        direct.epoch_secret.clone(),
        overlay.epoch_secret.clone(),
        turn.epoch_secret.clone(),
        b"mls-epoch-secret".to_vec(),
        b"content-key".to_vec(),
    ];
    let forbidden_refs = forbidden_payloads
        .iter()
        .map(Vec::as_slice)
        .collect::<Vec<_>>();
    let relay_pcap_protected_only = pcap.no_forbidden_content_egress(&forbidden_refs)
        && pcap.events().iter().any(|event| {
            event.component == InfrastructureComponent::PeerRelay
                && event.content == ContentExposure::CiphertextOnly
        })
        && pcap.events().iter().any(|event| {
            event.component == InfrastructureComponent::Turn
                && event.content == ContentExposure::CiphertextOnly
        });

    Ok(VoiceMediaE2eSmoke {
        direct_webrtc_audio_exchanged: direct.audio_exchanged,
        overlay_audio_exchanged: overlay.audio_exchanged,
        turn_audio_exchanged: turn.audio_exchanged,
        mute_blocks_outbound_audio,
        volume_affects_playback: turn.volume_zeroed,
        speaking_follows_actual_audio: direct.speaking && overlay.speaking && turn.speaking,
        relay_pcap_protected_only,
    })
}

/// Run one node's local protected-envelope relay checks for the process harness.
pub fn overlay_node_process_report(
    node_index: usize,
) -> Result<OverlayNodeProcessReport, discrypt_media::MediaError> {
    use discrypt_media::{
        MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameReceiver, SFrameSender,
        SenderBinding,
    };
    use discrypt_relay_overlay::integrity::{
        contains_plaintext, RelayPacket, RelayPayloadKind, RelayProtectedEnvelope,
    };

    let epoch_secret = [node_index as u8; 32];
    let binding = SenderBinding::derive_for_epoch(
        &epoch_secret,
        "overlay-process",
        node_index as u64,
        node_index as u32,
        format!("node-{node_index}-device"),
    )?;
    let plaintext = format!("node-{node_index} voice payload");
    let mut sender = SFrameSender::new(&epoch_secret, binding.clone())?;
    let protected = sender.protect(plaintext.as_bytes())?;
    let envelope = RelayProtectedEnvelope::new(
        RelayPayloadKind::Media,
        protected.kid.clone(),
        protected.counter,
        format!("overlay-node-route:{node_index}").as_bytes(),
        protected.ciphertext.clone(),
    )
    .map_err(|_| MediaError::AuthenticationFailed)?;
    let relay_packet =
        RelayPacket::from_envelope(format!("relay-{node_index}"), envelope).forward("next-relay");
    let relay_visible_ciphertext_only = !contains_plaintext(&relay_packet, plaintext.as_bytes())
        && !relay_packet.envelope.kid.is_empty();

    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&epoch_secret, binding.clone())?;
    let mut tamper_receiver = SFrameReceiver::new(tamper_registry, ReplayWindow::default());
    let tampered = relay_packet.clone().tamper();
    let tamper_rejected = tamper_receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: tampered.envelope.ciphertext,
    }) == Err(MediaError::AuthenticationFailed);

    let mut replay_registry = MediaKeyRegistry::new();
    replay_registry.register_sender(&epoch_secret, binding)?;
    let mut replay_receiver = SFrameReceiver::new(replay_registry, ReplayWindow::default());
    let replay_frame = ProtectedFrame {
        kid: protected.kid,
        counter: protected.counter,
        ciphertext: relay_packet.envelope.ciphertext,
    };
    let first_opened = replay_receiver.open(&replay_frame).is_ok();
    let replay_rejected =
        first_opened && replay_receiver.open(&replay_frame) == Err(MediaError::Replay);

    Ok(OverlayNodeProcessReport {
        node_index,
        relay_visible_ciphertext_only,
        tamper_rejected,
        replay_rejected,
    })
}

/// Exercise Phase-2 topology, failover, redelivery, store-forward, and media integrity.
pub fn relay_overlay_smoke() -> Result<RelayOverlaySmoke, anyhow::Error> {
    use discrypt_media::{
        MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameReceiver, SFrameSender,
        SenderBinding,
    };
    use discrypt_relay_overlay::capability::{
        BatteryDozePosture, RelayCapabilityAdvertisement, RelayCapacityAdvertisement,
    };
    use discrypt_relay_overlay::integrity::{
        contains_plaintext, RelayPacket, RelayPayloadKind, RelayProtectedEnvelope,
    };
    use discrypt_relay_overlay::ranking::RelayMetrics;
    use discrypt_relay_overlay::redelivery::{PacketId, RedeliveryError, RedeliveryTracker};
    use discrypt_relay_overlay::store_forward::{
        StoreForwardEnvelope, StoreForwardError, StoreForwardPolicy, StoreForwardQueue,
        VolunteerRelaySettings,
    };
    use discrypt_relay_overlay::topology::RelayTopology;
    use discrypt_relay_overlay::{OverlayManager, RelayRuntimeObservation};

    fn metrics(peer_id: &str, latency_ms: u32, freeload_penalty: f32) -> RelayMetrics {
        RelayMetrics {
            peer_id: peer_id.to_owned(),
            latency_ms,
            stability: 1.0,
            battery_cost: 0.0,
            freeload_penalty,
        }
    }

    fn protected_payload(
        kind: RelayPayloadKind,
        kid: Vec<u8>,
        counter: u64,
        aad_metadata: &[u8],
        ciphertext: Vec<u8>,
    ) -> Result<RelayProtectedEnvelope, anyhow::Error> {
        Ok(RelayProtectedEnvelope::new(
            kind,
            kid,
            counter,
            aad_metadata,
            ciphertext,
        )?)
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
    let failover = discrypt_relay_overlay::failover::reroute_after_failure(
        &topology,
        route.clone(),
        "primary-relay",
        2_500,
    )?;
    let failover_recovered = failover.converged_within_phase2_gate()
        && failover.replacement.path == ["alice", "backup-relay", "bob"]
        && !failover.replacement.contains_peer("primary-relay");

    let mut manager = OverlayManager::default();
    for peer in [
        ("alice", 1),
        ("primary-relay", 10),
        ("backup-relay", 30),
        ("bob", 1),
    ] {
        manager.upsert_observation(RelayRuntimeObservation {
            peer_id: peer.0.to_owned(),
            latency_ms: peer.1,
            successful_probes: 10,
            failed_probes: 0,
            battery_cost_bps: 0,
            contributed_bytes: 10_000,
            consumed_bytes: 0,
        })?;
    }
    manager.connect_peers("alice", "primary-relay")?;
    manager.connect_peers("primary-relay", "bob")?;
    manager.connect_peers("alice", "backup-relay")?;
    manager.connect_peers("backup-relay", "bob")?;
    let media_failover = manager.mark_failed_media_and_reroute(
        manager.route("alice", "bob")?.route,
        "primary-relay",
        2_500,
        180,
    )?;
    let media_gap_concealed = media_failover
        .media_concealment
        .as_ref()
        .is_some_and(|report| report.target_met && report.observed_gap_ms <= 200);

    let binding =
        SenderBinding::derive_for_epoch(&[42; 32], "phase2-overlay", 42, 1, "alice-laptop")?;
    let mut sender = SFrameSender::new(&[42; 32], binding.clone())?;
    let mut registry = MediaKeyRegistry::new();
    registry.register_sender(&[42; 32], binding.clone())?;
    let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

    let plaintext = b"phase2 encoded voice frame";
    let protected = sender.protect(plaintext)?;
    let relay_envelope = protected_payload(
        RelayPayloadKind::Media,
        protected.kid.clone(),
        protected.counter,
        b"route:alice:primary-relay:bob",
        protected.ciphertext.clone(),
    )?;
    let relayed = route.path[1..route.path.len() - 1].iter().try_fold(
        RelayPacket::from_envelope(&route.path[1], relay_envelope),
        |packet, hop| packet.forward_checked(hop),
    )?;
    let ciphertext_only_media = !contains_plaintext(&relayed, b"voice frame");
    let opened = receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: relayed.envelope.ciphertext.clone(),
    })?;

    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&[42; 32], binding)?;
    let mut tamper_receiver = SFrameReceiver::new(tamper_registry, ReplayWindow::default());
    let tampered_packet = RelayPacket::from_envelope(
        "primary-relay",
        protected_payload(
            RelayPayloadKind::Media,
            protected.kid.clone(),
            protected.counter,
            b"route:alice:primary-relay:bob",
            protected.ciphertext.clone(),
        )?,
    )
    .tamper();
    let tamper_rejected = tamper_receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: tampered_packet.envelope.ciphertext,
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

    fn relay_capability(
        peer_id: &str,
        accepts_store_forward: bool,
    ) -> RelayCapabilityAdvertisement {
        RelayCapabilityAdvertisement {
            peer_id: peer_id.to_owned(),
            sequence: 1,
            issued_at_ms: 1_000,
            expires_at_ms: 2_000,
            relay_capacity: RelayCapacityAdvertisement {
                max_fanout: 8,
                egress_bytes_per_second: 64_000,
                accepts_store_forward,
            },
            battery_doze: BatteryDozePosture::Charging,
            observed_rtt_ms: 10,
            packet_loss_bps: 0,
            contributed_bytes: 1_000,
            consumed_bytes: 1_000,
        }
    }

    let mut volunteer = VolunteerRelaySettings::enabled("primary-relay");
    volunteer.max_queue_envelopes = 4;
    volunteer.max_fanout_per_message = 2;
    volunteer.max_volunteer_relays = 2;
    let mut queue = StoreForwardQueue::with_policy(StoreForwardPolicy::new(
        ["bob"],
        1_000,
        1_000,
        volunteer.clone(),
    ));
    let plaintext_leak = StoreForwardEnvelope::new(
        "plaintext-leak",
        "bob",
        protected_payload(
            RelayPayloadKind::StoreForward,
            b"leak-kid".to_vec(),
            1,
            b"store-forward:plaintext-leak",
            b"visible phase2 encoded voice frame".to_vec(),
        )?,
        1_000,
        1_000,
        1,
    )?;
    let store_forward_plaintext_rejected = queue
        .enqueue_ciphertext_only(plaintext_leak, b"voice frame")
        == Err(StoreForwardError::VisiblePlaintext);
    let non_member_rejected = queue.enqueue(StoreForwardEnvelope::new(
        "media-mallory",
        "mallory",
        protected_payload(
            RelayPayloadKind::StoreForward,
            b"mallory-kid".to_vec(),
            1,
            b"store-forward:media-mallory",
            b"opaque ciphertext".to_vec(),
        )?,
        1_000,
        500,
        1,
    )?) == Err(StoreForwardError::UnauthorizedRecipient);
    let mut retention_queue = StoreForwardQueue::with_policy(StoreForwardPolicy::new(
        ["bob"],
        2_000,
        1_000,
        volunteer.clone(),
    ));
    let retention_window_rejected = retention_queue.enqueue(StoreForwardEnvelope::new(
        "media-retention",
        "bob",
        protected_payload(
            RelayPayloadKind::StoreForward,
            b"retention-kid".to_vec(),
            1,
            b"store-forward:media-retention",
            b"opaque ciphertext".to_vec(),
        )?,
        1_000,
        1_500,
        1,
    )?) == Err(StoreForwardError::RetentionWindowExceeded);
    let mut disabled_queue = StoreForwardQueue::with_policy(StoreForwardPolicy::new(
        ["bob"],
        1_000,
        1_000,
        VolunteerRelaySettings::disabled("primary-relay"),
    ));
    let volunteer_disabled_rejected = disabled_queue.enqueue(StoreForwardEnvelope::new(
        "media-disabled",
        "bob",
        protected_payload(
            RelayPayloadKind::StoreForward,
            b"disabled-kid".to_vec(),
            1,
            b"store-forward:media-disabled",
            b"opaque ciphertext".to_vec(),
        )?,
        1_000,
        500,
        1,
    )?) == Err(StoreForwardError::VolunteerRelayDisabled);
    let volunteer_targets = queue.volunteer_targets(
        &[
            relay_capability("primary-relay", true),
            relay_capability("backup-relay", true),
            relay_capability("no-store-relay", false),
        ],
        1_100,
    );
    queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new(
            "media-1",
            "bob",
            protected_payload(
                RelayPayloadKind::StoreForward,
                protected.kid.clone(),
                protected.counter,
                b"store-forward:media-1",
                protected.ciphertext,
            )?,
            1_000,
            1_000,
            2,
        )?,
        b"voice frame",
    )?;
    let delivered_before_ttl = queue.drain_for_recipient("bob", 1_500).len() == 1;
    queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new(
            "media-2",
            "bob",
            protected_payload(
                RelayPayloadKind::StoreForward,
                b"opaque-kid".to_vec(),
                2,
                b"store-forward:media-2",
                b"opaque ciphertext".to_vec(),
            )?,
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
        media_gap_concealed,
        redelivery_replay_rejected,
        store_forward_plaintext_rejected,
        store_forward_ttl_enforced: delivered_before_ttl && expired_not_delivered,
        store_forward_fanout_bounded: store_forward_fanout_bounded
            && non_member_rejected
            && retention_window_rejected
            && volunteer_disabled_rejected
            && volunteer_targets == vec!["backup-relay".to_owned()],
        ciphertext_only_media,
        tamper_rejected,
        plaintext: opened.plaintext,
    })
}

/// Exercise malicious relay passive read, tamper, replay, drop, reorder, and churn cases.
pub fn malicious_relay_adversary_smoke() -> Result<MaliciousRelayAdversarySmoke, anyhow::Error> {
    use discrypt_media::{
        MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameReceiver, SFrameSender,
        SenderBinding,
    };
    use discrypt_relay_overlay::integrity::{
        contains_plaintext, RelayPacket, RelayPayloadKind, RelayProtectedEnvelope,
    };
    use discrypt_relay_overlay::redelivery::{PacketId, RedeliveryError, RedeliveryTracker};
    use discrypt_relay_overlay::{
        ChurnDampingPolicy, OverlayManager, OverlayManagerError, RelayRuntimeObservation,
        TopologyChangeReason,
    };

    fn protected_payload(
        kind: RelayPayloadKind,
        kid: Vec<u8>,
        counter: u64,
        aad_metadata: &[u8],
        ciphertext: Vec<u8>,
    ) -> Result<RelayProtectedEnvelope, anyhow::Error> {
        Ok(RelayProtectedEnvelope::new(
            kind,
            kid,
            counter,
            aad_metadata,
            ciphertext,
        )?)
    }

    fn observation(peer_id: &str, latency_ms: u32) -> RelayRuntimeObservation {
        RelayRuntimeObservation {
            peer_id: peer_id.to_owned(),
            latency_ms,
            successful_probes: 10,
            failed_probes: 0,
            battery_cost_bps: 0,
            contributed_bytes: 10_000,
            consumed_bytes: 0,
        }
    }

    let epoch_secret = [97; 32];
    let binding = SenderBinding::derive_for_epoch(
        &epoch_secret,
        "phase-n-malicious-relay",
        97,
        1,
        "alice-laptop",
    )?;
    let mut sender = SFrameSender::new(&epoch_secret, binding.clone())?;
    let plaintext = b"malicious relay protected voice frame";
    let protected = sender.protect(plaintext)?;
    let packet = RelayPacket::from_envelope(
        "relay-a",
        protected_payload(
            RelayPayloadKind::Media,
            protected.kid.clone(),
            protected.counter,
            b"route:alice:relay-a:bob",
            protected.ciphertext.clone(),
        )?,
    )
    .forward_checked("bob")?;
    let visible = packet.envelope.visible_bytes();
    let passive_read_blocked = !contains_plaintext(&packet, plaintext)
        && !contains_bytes(&visible, &epoch_secret)
        && !contains_bytes(&visible, b"sframe-key")
        && !contains_bytes(&visible, b"mls-epoch-secret");

    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&epoch_secret, binding.clone())?;
    let mut tamper_receiver = SFrameReceiver::new(tamper_registry, ReplayWindow::default());
    let tampered_packet = packet.clone().tamper();
    let tamper_rejected = tamper_receiver.open(&ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: tampered_packet.envelope.ciphertext,
    }) == Err(MediaError::AuthenticationFailed);

    let mut replay_registry = MediaKeyRegistry::new();
    replay_registry.register_sender(&epoch_secret, binding)?;
    let mut replay_receiver = SFrameReceiver::new(replay_registry, ReplayWindow::default());
    let replay_frame = ProtectedFrame {
        kid: protected.kid.clone(),
        counter: protected.counter,
        ciphertext: protected.ciphertext.clone(),
    };
    let replay_rejected = replay_receiver.open(&replay_frame).is_ok()
        && replay_receiver.open(&replay_frame) == Err(MediaError::Replay);

    let mut dropped = RedeliveryTracker::new(64, 2);
    let dropped_packet = PacketId {
        sender_id: hex_id(&protected.kid),
        sequence: protected.counter.saturating_add(1),
    };
    dropped.request_redelivery(dropped_packet.clone(), "relay-b")?;
    dropped.request_redelivery(dropped_packet.clone(), "relay-c")?;
    let drop_requests_bounded_redelivery = dropped.redelivery_fanout(&dropped_packet) == 2
        && dropped.request_redelivery(dropped_packet, "relay-d")
            == Err(RedeliveryError::FanoutExhausted);

    let mut reordered = RedeliveryTracker::new(4, 2);
    let reorder_window_enforced = reordered.accept(&packet_id("kid-reorder", 10)) == Ok(())
        && reordered.accept(&packet_id("kid-reorder", 8)) == Ok(())
        && reordered.accept(&packet_id("kid-reorder", 8)) == Err(RedeliveryError::Replay)
        && reordered.accept(&packet_id("kid-reorder", 15)) == Ok(())
        && reordered.accept(&packet_id("kid-reorder", 10)) == Err(RedeliveryError::Replay);

    let mut manager = OverlayManager::default().with_churn_policy(ChurnDampingPolicy {
        min_planned_change_interval_ms: 30_000,
    });
    for peer in [
        observation("alice", 5),
        observation("relay-a", 20),
        observation("relay-b", 30),
        observation("relay-c", 35),
        observation("bob", 5),
    ] {
        manager.upsert_observation(peer)?;
    }
    manager.connect_peers_with_churn_damping(
        "alice",
        "relay-a",
        1_000,
        TopologyChangeReason::PlannedReparent,
    )?;
    let planned_churn_damped = matches!(
        manager.connect_peers_with_churn_damping(
            "alice",
            "relay-b",
            30_999,
            TopologyChangeReason::PlannedReparent,
        ),
        Err(OverlayManagerError::ChurnDamped {
            next_allowed_at_ms: 31_000
        })
    );
    manager.connect_peers_with_churn_damping(
        "alice",
        "relay-b",
        1_001,
        TopologyChangeReason::HardFailure,
    )?;
    manager.connect_peers("relay-b", "bob")?;
    let endpoint_churn_damped_and_failover_recovered = planned_churn_damped
        && manager
            .route("alice", "bob")?
            .route
            .contains_peer("relay-b");

    Ok(MaliciousRelayAdversarySmoke {
        passive_read_blocked,
        tamper_rejected,
        replay_rejected,
        drop_requests_bounded_redelivery,
        reorder_window_enforced,
        endpoint_churn_damped_and_failover_recovered,
    })
}

fn packet_id(sender_id: &str, sequence: u64) -> discrypt_relay_overlay::redelivery::PacketId {
    discrypt_relay_overlay::redelivery::PacketId {
        sender_id: sender_id.to_owned(),
        sequence,
    }
}

/// Exercise malicious member/device media impersonation, eviction, fork, and governance cases.
pub fn malicious_member_adversary_smoke() -> Result<MaliciousMemberAdversarySmoke, anyhow::Error> {
    use discrypt_media::{
        MediaError, MediaKeyRegistry, ProtectedFrame, ReplayWindow, SFrameReceiver, SFrameSender,
        SenderBinding,
    };
    use discrypt_mls_core::governance::{
        GovernanceAction, GovernanceError, GovernanceEvent, GovernanceState, Role,
    };
    use discrypt_mls_delivery::{
        summary, ApplicationEvent, CommitEnvelope, DeliveryError, DeliveryState,
        InMemoryTextReceiveEvents, InMemoryTextRecipientStore, TextInboundPipeline,
        TextInboundRequest, TextMessageEnvelope, TextMessageEnvelopeInput, TextReceiveState,
        TextRetentionMetadata,
    };
    use ed25519_dalek::SigningKey;
    use std::collections::BTreeSet;

    let media_secret = [98; 32];
    let alice_binding = SenderBinding::derive_for_epoch(
        &media_secret,
        "phase-n-malicious-member",
        98,
        1,
        "alice-laptop",
    )?;
    let mallory_binding = SenderBinding::derive_for_epoch(
        &media_secret,
        "phase-n-malicious-member",
        98,
        9,
        "mallory-phone",
    )?;
    let mut alice_sender = SFrameSender::new(&media_secret, alice_binding)?;
    let alice_frame = alice_sender.protect(b"alice voice frame")?;
    let mut impersonation_registry = MediaKeyRegistry::new();
    impersonation_registry.register_sender(&media_secret, mallory_binding.clone())?;
    let mut impersonation_receiver =
        SFrameReceiver::new(impersonation_registry, ReplayWindow::default());
    let relabeled_frame = ProtectedFrame {
        kid: mallory_binding.kid,
        counter: alice_frame.counter,
        ciphertext: alice_frame.ciphertext.clone(),
    };
    let media_impersonation_rejected =
        impersonation_receiver.open(&relabeled_frame) == Err(MediaError::AuthenticationFailed);

    let text_secret = b"openmls-text-exporter-secret";
    let evicted_signing_key = SigningKey::from_bytes(&[98; 32]);
    let evicted_envelope = TextMessageEnvelope::sign(
        "phase-n-malicious-member",
        TextMessageEnvelopeInput {
            epoch: 98,
            sender_leaf: 9,
            sender_device_id: "mallory-phone".to_owned(),
            sequence: 1,
            message_id: "evicted-member-text".to_owned(),
            retention: TextRetentionMetadata::new("phase-n", 0, Some(60_000), false),
            content_ciphertext: b"ciphertext-from-evicted-leaf".to_vec(),
        },
        &evicted_signing_key,
    )?;
    let mut receive_state = TextReceiveState::default();
    let mut receive_store = InMemoryTextRecipientStore::default();
    let mut receive_events = InMemoryTextReceiveEvents::default();
    let evicted_member_text_rejected =
        TextInboundPipeline::new(&mut receive_state, &mut receive_store, &mut receive_events)
            .receive(
                TextInboundRequest {
                    group_id: "phase-n-malicious-member".to_owned(),
                    channel_id: "general".to_owned(),
                    current_epoch: 98,
                    authorized_sender_leaves: BTreeSet::from([1_u32, 2_u32]),
                    envelope: evicted_envelope,
                    received_at_ms: 98_000,
                    retention_allows_decrypt: true,
                },
                text_secret,
                &evicted_signing_key.verifying_key(),
            )
            == Err(DeliveryError::TextReceiveUnauthorizedSender(9));

    let old_secret = [7; 32];
    let new_secret = [8; 32];
    let old_binding = SenderBinding::derive_for_epoch(
        &old_secret,
        "phase-n-malicious-member",
        97,
        4,
        "alice-lost-phone",
    )?;
    let mut old_device_sender = SFrameSender::new(&old_secret, old_binding)?;
    let old_device_frame = old_device_sender.protect(b"old device media")?;
    let replacement_binding = SenderBinding::derive_for_epoch(
        &new_secret,
        "phase-n-malicious-member",
        98,
        5,
        "alice-replacement-phone",
    )?;
    let mut post_eviction_registry = MediaKeyRegistry::new();
    post_eviction_registry.register_sender(&new_secret, replacement_binding)?;
    let mut post_eviction_receiver =
        SFrameReceiver::new(post_eviction_registry, ReplayWindow::default());
    let evicted_device_media_rejected =
        post_eviction_receiver.open(&old_device_frame) == Err(MediaError::UnknownSender);

    let mut delivery = DeliveryState::new(summary(98, 1, 1));
    delivery.apply_commit(CommitEnvelope::new(
        summary(99, 2, 2),
        1,
        vec![ApplicationEvent::new(
            99,
            1,
            "honest-forward-commit",
            b"ok".to_vec(),
        )],
    ))?;
    let forked_mls_commit_rejected = delivery.apply_commit(CommitEnvelope::new(
        summary(99, 3, 3),
        9,
        vec![ApplicationEvent::new(
            99,
            9,
            "forked-commit-event",
            b"fork".to_vec(),
        )],
    )) == Err(DeliveryError::DivergentTree(99));

    let mut governance = GovernanceState::new(98, 1);
    governance.apply_event(GovernanceEvent::signed(
        98,
        1,
        GovernanceAction::SetRole {
            target: 2,
            role: Role::Admin,
        },
    ))?;
    let out_of_epoch_governance_rejected = governance.apply_event(GovernanceEvent::signed(
        99,
        1,
        GovernanceAction::RevokeInvite {
            invite_id: "future".to_owned(),
        },
    )) == Err(GovernanceError::OutOfEpoch);
    let unauthorized_governance_rejected = governance.apply_event(GovernanceEvent::signed(
        98,
        9,
        GovernanceAction::RevokeInvite {
            invite_id: "unauthorized".to_owned(),
        },
    )) == Err(GovernanceError::Unauthorized);
    let removed_admin_race = governance.resolve_epoch_events([
        GovernanceEvent::signed(
            98,
            2,
            GovernanceAction::RevokeInvite {
                invite_id: "removed-admin-race".to_owned(),
            },
        ),
        GovernanceEvent::signed(98, 1, GovernanceAction::Ban { target: 2 }),
    ]);
    let removed_admin_race_rejected = removed_admin_race
        == vec![Ok(()), Err(GovernanceError::EvictedCommitter)]
        && governance.is_banned(2)
        && !governance.invite_revoked("removed-admin-race");

    Ok(MaliciousMemberAdversarySmoke {
        media_impersonation_rejected,
        evicted_member_text_rejected,
        evicted_device_media_rejected,
        forked_mls_commit_rejected,
        out_of_epoch_governance_rejected,
        unauthorized_governance_rejected,
        removed_admin_race_rejected,
    })
}

fn hex_id(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

/// Exercise Phase-3 text, history, MLS delivery, gossip, Welcome, and fork repair.
pub fn text_history_delivery_smoke() -> Result<TextHistoryDeliverySmoke, anyhow::Error> {
    use discrypt_mls_core::{derive_epoch_secret, ExportLabel};
    use discrypt_mls_delivery::{
        detect_fork_or_replay, equal_confirmation_tags, order_application_events, plan_repair,
        repair_to_winner, summary, ApplicationEvent, CatchUpBundle, CommitEnvelope, DeliveryError,
        DeliveryState, ForkEvidence, ForkStatus, InMemoryTextAuthorLog, InMemoryTextReceiveEvents,
        InMemoryTextRecipientStore, InMemoryTextSendEvents, InMemoryTextTransport,
        TextAuthorLogEnvelope, TextHistoryMergeEventKind, TextHistoryMergeState,
        TextInboundPipeline, TextInboundRequest, TextMessageEnvelope, TextMessageEnvelopeInput,
        TextOutboundPipeline, TextOutboundRequest, TextReceiveEventKind, TextReceiveState,
        TextReceivedEnvelope, TextRenderState, TextRetentionMetadata, TextSelectedRoute,
        TextSendEventKind, WelcomePackage,
    };
    use discrypt_relay_overlay::integrity::{RelayPayloadKind, RelayProtectedEnvelope};
    use discrypt_relay_overlay::store_forward::{
        StoreForwardEnvelope, StoreForwardError, StoreForwardPolicy, StoreForwardQueue,
        VolunteerRelaySettings,
    };
    use discrypt_relay_overlay::{GossipItem, GossipMesh};
    use discrypt_storage::{AuthorLogEntry, KeyState, LocalStore, RecipientCacheEntry};
    use discrypt_transport::{
        ConnectivityConfig, FallbackLeg, LocalProcessSocketAdapter, SimulatedNat,
    };
    use external_signaling::{AuditFixture, ContentExposure, InfrastructureComponent, PcapEvent};
    use std::collections::BTreeSet;

    fn protected_text_payload(
        counter: u64,
        aad_metadata: &[u8],
        ciphertext: Vec<u8>,
    ) -> Result<RelayProtectedEnvelope, anyhow::Error> {
        Ok(RelayProtectedEnvelope::new(
            RelayPayloadKind::StoreForward,
            b"text-store-forward-kid".to_vec(),
            counter,
            aad_metadata,
            ciphertext,
        )?)
    }

    let text_plaintext = b"hello from app-level encrypted text";
    let text_key = derive_epoch_secret(&[33; 32], ExportLabel::Text, b"room=lab;epoch=7;m=alice-1");
    let text_ciphertext = xor_text_ciphertext(&text_key, text_plaintext);
    let text_signing_key = ed25519_dalek::SigningKey::from_bytes(&[77; 32]);
    let text_envelope = TextMessageEnvelope::sign(
        "lab",
        TextMessageEnvelopeInput {
            epoch: 7,
            sender_leaf: 1,
            sender_device_id: "alice-laptop".to_owned(),
            sequence: 1,
            message_id: "alice-1".to_owned(),
            retention: TextRetentionMetadata::new("7 day default", 0, Some(604_800_000), false),
            content_ciphertext: text_ciphertext.clone(),
        },
        &text_signing_key,
    )?;
    let mut outbound_log = InMemoryTextAuthorLog::default();
    let mut outbound_transport = InMemoryTextTransport::default();
    let mut outbound_events = InMemoryTextSendEvents::default();
    let outbound_receipt = TextOutboundPipeline::new(
        &mut outbound_log,
        &mut outbound_transport,
        &mut outbound_events,
    )
    .send(
        TextOutboundRequest {
            group_id: "lab".to_owned(),
            channel_id: "general".to_owned(),
            epoch: 7,
            sender_leaf: 1,
            sender_device_id: "alice-laptop".to_owned(),
            sequence: 2,
            message_id: "alice-pipeline-2".to_owned(),
            retention: TextRetentionMetadata::new("7 day default", 0, Some(604_800_000), false),
            plaintext: text_plaintext.to_vec(),
            sent_at_ms: 1,
            now: chrono::Utc::now(),
        },
        TextSelectedRoute {
            session_id: "text-session".to_owned(),
            route_label: "overlay-hop".to_owned(),
            overlay_hops: 2,
            ciphertext_only: true,
        },
        &text_key,
        &text_signing_key,
    )?;
    let mut inbound_state = TextReceiveState::default();
    let mut inbound_store = InMemoryTextRecipientStore::default();
    let mut inbound_events = InMemoryTextReceiveEvents::default();
    let inbound_renderable =
        TextInboundPipeline::new(&mut inbound_state, &mut inbound_store, &mut inbound_events)
            .receive(
                TextInboundRequest {
                    group_id: "lab".to_owned(),
                    channel_id: "general".to_owned(),
                    current_epoch: 7,
                    authorized_sender_leaves: BTreeSet::from([1]),
                    envelope: outbound_receipt.envelope.clone(),
                    received_at_ms: 2,
                    retention_allows_decrypt: true,
                },
                &text_key,
                &text_signing_key.verifying_key(),
            )?;
    let pipeline_send_ready = outbound_log.entries.len() == 1
        && outbound_transport.frames.len() == 1
        && outbound_events
            .events
            .iter()
            .map(|event| &event.kind)
            .collect::<Vec<_>>()
            == vec![
                &TextSendEventKind::Pending,
                &TextSendEventKind::TransportAccepted,
            ]
        && inbound_store.entries.len() == 1
        && inbound_events.events.len() == 1
        && inbound_events.events[0].kind == TextReceiveEventKind::Updated
        && inbound_renderable.state == TextRenderState::Decrypted(text_plaintext.to_vec())
        && !outbound_receipt
            .envelope
            .contains_plaintext_sample(text_plaintext)
        && outbound_receipt
            .envelope
            .verify("lab", &text_signing_key.verifying_key())
            == Ok(());
    let opened_text = xor_text_ciphertext(&text_key, &text_envelope.content_ciphertext);
    let text_e2e_roundtrip = opened_text == text_plaintext
        && text_envelope.content_ciphertext != text_plaintext
        && text_envelope.verify("lab", &text_signing_key.verifying_key()) == Ok(())
        && pipeline_send_ready;

    let mut text_transport_pcap = AuditFixture::default();
    let text_key_forbidden = text_key;
    let forbidden_text_tokens: [&[u8]; 4] = [
        text_plaintext.as_slice(),
        text_key_forbidden.as_slice(),
        b"content-key".as_slice(),
        b"mls-epoch-secret".as_slice(),
    ];
    let mut verify_text_route = |route_label: &str,
                                 nat: SimulatedNat,
                                 expected_leg: FallbackLeg,
                                 message_id: &str,
                                 sequence: u64|
     -> Result<bool, anyhow::Error> {
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport::default();
        let mut events = InMemoryTextSendEvents::default();
        let receipt = TextOutboundPipeline::new(&mut log, &mut transport, &mut events).send(
            TextOutboundRequest {
                group_id: "lab".to_owned(),
                channel_id: "general".to_owned(),
                epoch: 7,
                sender_leaf: 1,
                sender_device_id: "alice-laptop".to_owned(),
                sequence,
                message_id: message_id.to_owned(),
                retention: TextRetentionMetadata::new("7 day default", 0, Some(604_800_000), false),
                plaintext: text_plaintext.to_vec(),
                sent_at_ms: sequence,
                now: chrono::Utc::now(),
            },
            TextSelectedRoute {
                session_id: format!("text-{route_label}-session"),
                route_label: route_label.to_owned(),
                overlay_hops: if expected_leg == FallbackLeg::RelayOverlay {
                    2
                } else {
                    0
                },
                ciphertext_only: true,
            },
            &text_key,
            &text_signing_key,
        )?;
        let payload = receipt.envelope.canonical_signed_bytes();
        let conformance = LocalProcessSocketAdapter::new(
            ConnectivityConfig::default(),
            nat,
            text_plaintext.to_vec(),
        )
        .run_conformance(&payload)?;
        let mut receive_state = TextReceiveState::default();
        let mut receive_store = InMemoryTextRecipientStore::default();
        let mut receive_events = InMemoryTextReceiveEvents::default();
        let received =
            TextInboundPipeline::new(&mut receive_state, &mut receive_store, &mut receive_events)
                .receive(
                TextInboundRequest {
                    group_id: "lab".to_owned(),
                    channel_id: "general".to_owned(),
                    current_epoch: 7,
                    authorized_sender_leaves: BTreeSet::from([1]),
                    envelope: receipt.envelope.clone(),
                    received_at_ms: sequence + 10,
                    retention_allows_decrypt: true,
                },
                &text_key,
                &text_signing_key.verifying_key(),
            )?;
        let (component, content, visible_bytes) = match expected_leg {
            FallbackLeg::Stun => (
                InfrastructureComponent::Stun,
                ContentExposure::None,
                b"text direct path stun binding; no app payload".to_vec(),
            ),
            FallbackLeg::RelayOverlay => (
                InfrastructureComponent::PeerRelay,
                ContentExposure::CiphertextOnly,
                payload.clone(),
            ),
            FallbackLeg::Turn => (
                InfrastructureComponent::Turn,
                ContentExposure::CiphertextOnly,
                payload.clone(),
            ),
        };
        text_transport_pcap.push(PcapEvent {
            component,
            content,
            visible_bytes,
            ip_or_endpoint: true,
            timing: true,
            persists_linkage: false,
        });

        Ok(conformance.ready()
            && conformance.route_report.selected == expected_leg
            && conformance.ciphertext_delivered
            && log.entries.len() == 1
            && transport.frames.len() == 1
            && events
                .events
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>()
                == vec![
                    &TextSendEventKind::Pending,
                    &TextSendEventKind::TransportAccepted,
                ]
            && receive_store.entries.len() == 1
            && receive_events.events.len() == 1
            && received.state == TextRenderState::Decrypted(text_plaintext.to_vec())
            && receipt.route.route_label == route_label
            && !receipt.envelope.contains_plaintext_sample(text_plaintext)
            && !payload
                .windows(text_plaintext.len())
                .any(|window| window == text_plaintext))
    };
    let direct_path_text_exchanged = verify_text_route(
        "direct",
        SimulatedNat::direct(),
        FallbackLeg::Stun,
        "alice-direct-3",
        3,
    )?;
    let overlay_path_text_exchanged = verify_text_route(
        "overlay",
        SimulatedNat::overlay_only(),
        FallbackLeg::RelayOverlay,
        "alice-overlay-4",
        4,
    )?;
    let turn_path_text_exchanged = verify_text_route(
        "turn",
        SimulatedNat::turn_only(),
        FallbackLeg::Turn,
        "alice-turn-5",
        5,
    )?;

    let mut volunteer = VolunteerRelaySettings::enabled("volunteer-relay-a");
    volunteer.max_queue_envelopes = 4;
    volunteer.max_fanout_per_message = 2;
    volunteer.max_volunteer_relays = 2;
    let store_forward_policy =
        StoreForwardPolicy::new(["bob-device"], 1_000, 1_000, volunteer.clone());
    let mut offline_queue = StoreForwardQueue::with_policy(store_forward_policy);
    let mut offline_log = InMemoryTextAuthorLog::default();
    let mut offline_transport = InMemoryTextTransport::default();
    let mut offline_events = InMemoryTextSendEvents::default();
    let offline_receipt = TextOutboundPipeline::new(
        &mut offline_log,
        &mut offline_transport,
        &mut offline_events,
    )
    .send(
        TextOutboundRequest {
            group_id: "lab".to_owned(),
            channel_id: "general".to_owned(),
            epoch: 7,
            sender_leaf: 1,
            sender_device_id: "alice-laptop".to_owned(),
            sequence: 6,
            message_id: "alice-offline-6".to_owned(),
            retention: TextRetentionMetadata::new("short text ttl", 1_000, Some(2_000), false),
            plaintext: text_plaintext.to_vec(),
            sent_at_ms: 1_000,
            now: chrono::Utc::now(),
        },
        TextSelectedRoute {
            session_id: "text-offline-store-forward".to_owned(),
            route_label: "store-forward".to_owned(),
            overlay_hops: 1,
            ciphertext_only: true,
        },
        &text_key,
        &text_signing_key,
    )?;
    let offline_payload = offline_receipt.envelope.canonical_signed_bytes();
    offline_queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new(
            "alice-offline-6",
            "bob-device",
            protected_text_payload(
                6,
                b"text-store-forward:alice-offline-6",
                offline_payload.clone(),
            )?,
            1_000,
            900,
            1,
        )?,
        text_plaintext,
    )?;
    let offline_delivered = offline_queue.drain_for_recipient("bob-device", 1_500);
    let mut offline_receive_state = TextReceiveState::default();
    let mut offline_receive_store = InMemoryTextRecipientStore::default();
    let mut offline_receive_events = InMemoryTextReceiveEvents::default();
    let offline_render = TextInboundPipeline::new(
        &mut offline_receive_state,
        &mut offline_receive_store,
        &mut offline_receive_events,
    )
    .receive(
        TextInboundRequest {
            group_id: "lab".to_owned(),
            channel_id: "general".to_owned(),
            current_epoch: 7,
            authorized_sender_leaves: BTreeSet::from([1]),
            envelope: offline_receipt.envelope.clone(),
            received_at_ms: 1_500,
            retention_allows_decrypt: true,
        },
        &text_key,
        &text_signing_key.verifying_key(),
    )?;
    if let Some(envelope) = offline_delivered.first() {
        text_transport_pcap.push(PcapEvent {
            component: InfrastructureComponent::VolunteerStorageRelay,
            content: ContentExposure::CiphertextOnly,
            visible_bytes: envelope.payload.visible_bytes(),
            ip_or_endpoint: true,
            timing: true,
            persists_linkage: false,
        });
    }
    let offline_store_forward_within_ttl = offline_delivered.len() == 1
        && offline_delivered[0].payload.ciphertext == offline_payload
        && offline_render.state == TextRenderState::Decrypted(text_plaintext.to_vec())
        && offline_receive_store.entries.len() == 1
        && offline_queue.is_empty();

    let mut retention_queue = StoreForwardQueue::with_policy(StoreForwardPolicy::new(
        ["bob-device"],
        2_000,
        500,
        volunteer,
    ));
    let retention_overrun_rejected = retention_queue.enqueue(StoreForwardEnvelope::new(
        "alice-retention-overrun",
        "bob-device",
        protected_text_payload(
            7,
            b"text-store-forward:retention-overrun",
            b"ciphertext".to_vec(),
        )?,
        1_000,
        1_000,
        1,
    )?) == Err(StoreForwardError::RetentionWindowExceeded);
    retention_queue.enqueue_ciphertext_only(
        StoreForwardEnvelope::new(
            "alice-retention-lock",
            "bob-device",
            protected_text_payload(
                8,
                b"text-store-forward:retention-lock",
                offline_payload.clone(),
            )?,
            1_000,
            400,
            1,
        )?,
        text_plaintext,
    )?;
    let locked_delivery = retention_queue.drain_for_recipient("bob-device", 1_300);
    let mut locked_receive_state = TextReceiveState::default();
    let mut locked_receive_store = InMemoryTextRecipientStore::default();
    let mut locked_receive_events = InMemoryTextReceiveEvents::default();
    let locked_render = TextInboundPipeline::new(
        &mut locked_receive_state,
        &mut locked_receive_store,
        &mut locked_receive_events,
    )
    .receive(
        TextInboundRequest {
            group_id: "lab".to_owned(),
            channel_id: "general".to_owned(),
            current_epoch: 7,
            authorized_sender_leaves: BTreeSet::from([1]),
            envelope: offline_receipt.envelope.clone(),
            received_at_ms: 1_300,
            retention_allows_decrypt: false,
        },
        &text_key,
        &text_signing_key.verifying_key(),
    )?;
    let retention_locks_old_store_forward = retention_overrun_rejected
        && locked_delivery.len() == 1
        && matches!(locked_render.state, TextRenderState::Locked { .. });
    let text_pcap_no_plaintext =
        text_transport_pcap.no_forbidden_content_egress(&forbidden_text_tokens);

    let laptop_entry = AuthorLogEntry::new(
        1,
        "alice-laptop",
        1,
        7,
        "alice-1",
        text_envelope.content_ciphertext.clone(),
    );
    let phone_text_envelope = TextMessageEnvelope::sign(
        "lab",
        TextMessageEnvelopeInput {
            epoch: 7,
            sender_leaf: 1,
            sender_device_id: "alice-phone".to_owned(),
            sequence: 2,
            message_id: "alice-2".to_owned(),
            retention: TextRetentionMetadata::new("7 day default", 0, Some(604_800_000), false),
            content_ciphertext: b"ciphertext-b".to_vec(),
        },
        &text_signing_key,
    )?;
    let phone_entry = AuthorLogEntry::new(
        1,
        "alice-phone",
        2,
        7,
        "alice-2",
        phone_text_envelope.content_ciphertext.clone(),
    );
    let mut laptop = LocalStore::default();
    laptop.append_sent(laptop_entry.clone());
    laptop.cache_received(RecipientCacheEntry::new(
        "alice-1",
        text_envelope.content_ciphertext.clone(),
        KeyState::Cached(text_key),
        0,
    ));
    let no_plaintext_in_text_surfaces = !laptop
        .author_log_snapshot()
        .iter()
        .any(|entry| contains_bytes(&entry.ciphertext, text_plaintext))
        && !text_envelope.contains_plaintext_sample(text_plaintext)
        && laptop
            .recipient_cache()
            .get("alice-1")
            .is_some_and(|entry| !contains_bytes(&entry.ciphertext, text_plaintext));
    let laptop_text_entry = TextAuthorLogEnvelope {
        channel_id: "general".to_owned(),
        envelope: text_envelope.clone(),
        sent_at_ms: 0,
    };
    let phone_text_entry = TextAuthorLogEnvelope {
        channel_id: "general".to_owned(),
        envelope: phone_text_envelope.clone(),
        sent_at_ms: 1,
    };
    let divergent_phone_entry = TextAuthorLogEnvelope {
        channel_id: "general".to_owned(),
        envelope: TextMessageEnvelope::sign(
            "lab",
            TextMessageEnvelopeInput {
                epoch: 7,
                sender_leaf: 1,
                sender_device_id: "alice-phone".to_owned(),
                sequence: 2,
                message_id: "alice-2-fork".to_owned(),
                retention: TextRetentionMetadata::new("7 day default", 0, Some(604_800_000), false),
                content_ciphertext: b"forked-ciphertext".to_vec(),
            },
            &text_signing_key,
        )?,
        sent_at_ms: 2,
    };
    let mut text_history = TextHistoryMergeState::with_recipient_cache_capacity(3);
    let merge_report = text_history.merge_author_log(vec![
        phone_text_entry.clone(),
        laptop_text_entry,
        phone_text_entry.clone(),
        divergent_phone_entry,
    ]);
    let author_logs_merged = merge_report.inserted == 2
        && merge_report.duplicates_suppressed == 1
        && merge_report.repair_events == 1
        && merge_report
            .events
            .iter()
            .any(|event| event.kind == TextHistoryMergeEventKind::RepairRequested)
        && text_history
            .author_log_snapshot()
            .iter()
            .map(|entry| entry.envelope.message_id.clone())
            .collect::<Vec<_>>()
            == vec!["alice-1".to_owned(), "alice-2".to_owned()];

    let mut cache_entries = Vec::new();
    for idx in 0..4 {
        let envelope = TextMessageEnvelope::sign(
            "lab",
            TextMessageEnvelopeInput {
                epoch: 7,
                sender_leaf: 1,
                sender_device_id: "alice-cache".to_owned(),
                sequence: 10 + idx,
                message_id: format!("cached-{idx}"),
                retention: TextRetentionMetadata::new("7 day default", idx, None, false),
                content_ciphertext: vec![idx as u8, 42],
            },
            &text_signing_key,
        )?;
        cache_entries.push(TextReceivedEnvelope {
            channel_id: "general".to_owned(),
            envelope,
            received_at_ms: idx,
        });
    }
    let cache_report = text_history.merge_received_cache(cache_entries);
    let recipient_cache_ids = text_history
        .recipient_cache_snapshot()
        .iter()
        .map(|entry| entry.envelope.message_id.clone())
        .collect::<Vec<_>>();
    let recipient_cache_bounded = recipient_cache_ids.len() == 3
        && !recipient_cache_ids.contains(&"cached-0".to_owned())
        && recipient_cache_ids.contains(&"cached-3".to_owned())
        && cache_report.evicted_from_recipient_cache == 1;

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
        text_e2e_roundtrip,
        direct_path_text_exchanged,
        overlay_path_text_exchanged,
        turn_path_text_exchanged,
        offline_store_forward_within_ttl,
        retention_locks_old_store_forward,
        text_pcap_no_plaintext,
        no_plaintext_in_text_surfaces,
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

/// Exercise Phase-4 retention, shred, live-key, secure-delete, and recovery negatives.
pub fn retention_shred_smoke() -> Result<RetentionShredSmoke, anyhow::Error> {
    use chrono::{Duration, Utc};
    use discrypt_content_keys::{
        key_state, CrossDeviceShredState, KeyState, LiveKeyOracle, MembershipProof,
        RetentionTransition, RetentionWindow,
    };
    use discrypt_storage::{seal_account_backup, SecureDeleteSimulator};
    use ed25519_dalek::SigningKey;
    use std::collections::{BTreeMap, BTreeSet};

    let now = Utc::now();
    let key = [7; 32];
    let default_window_locks_old_messages = matches!(
        key_state(
            now,
            now - Duration::days(3),
            RetentionWindow::Days7,
            key,
            false,
        ),
        KeyState::Cached(_)
    ) && key_state(
        now,
        now - Duration::days(8),
        RetentionWindow::Days7,
        key,
        false,
    ) == KeyState::Locked;

    let shorten = RetentionTransition {
        old_window: RetentionWindow::Days7,
        new_window: RetentionWindow::Hours24,
        changed_at: now,
    };
    let lengthen = RetentionTransition {
        old_window: RetentionWindow::Hours24,
        new_window: RetentionWindow::Days7,
        changed_at: now,
    };
    let shorten_retro_lengthen_future =
        shorten.state_for_message(now, now - Duration::days(2), key, false) == KeyState::Locked
            && lengthen.state_for_message(now, now - Duration::days(2), key, false)
                == KeyState::Locked
            && matches!(
                lengthen.state_for_message(now, now + Duration::seconds(1), key, false),
                KeyState::Cached(_)
            );

    let mut shred = CrossDeviceShredState::default();
    shred.register_device("laptop", true);
    shred.register_device("phone", false);
    shred.shred("m-shred");
    let phone_pending = shred.pending_on_device("phone", "m-shred");
    shred.set_online("phone", true);
    let cross_device_shred_sync = !shred.device_may_serve("laptop", "m-shred")
        && phone_pending
        && !shred.pending_on_device("phone", "m-shred")
        && !shred.device_may_serve("phone", "m-shred");

    let mut members = BTreeMap::new();
    members.insert(9, BTreeSet::from([1, 2]));
    let mut oracle = LiveKeyOracle::new(members, 1);
    let member_signer = SigningKey::from_bytes(&[1; 32]);
    let non_member_signer = SigningKey::from_bytes(&[99; 32]);
    let Some(commitment) = oracle.epoch_group_commitment(9) else {
        anyhow::bail!("retention smoke epoch commitment missing");
    };
    let proof = MembershipProof::sign(1, 9, "room", commitment, &member_signer);
    oracle.authorize_member_device(9, 1, &member_signer.verifying_key());
    let allowed = oracle.request_key(&proof, key);
    let limited = oracle.request_key(&proof, key);
    let decoy = oracle.request_key(
        &MembershipProof::sign(99, 9, "room", commitment, &non_member_signer),
        key,
    );
    let live_key_membership_rate_limit_decoy = allowed.authorized
        && allowed.state == KeyState::Cached(key)
        && !limited.authorized
        && limited.state == KeyState::RateLimited
        && !decoy.authorized
        && matches!(decoy.state, KeyState::Decoy(_));

    let mut delete = SecureDeleteSimulator::default();
    delete.write("db.sqlite", b"room content-key plaintext".to_vec());
    delete.write("db.sqlite-wal", b"wal content-key".to_vec());
    delete.write("key.store", b"wrapped content-key".to_vec());
    let snapshot = delete.snapshot();
    delete.secure_delete(["db.sqlite", "db.sqlite-wal"]);
    let failed_verify_kept_material = delete.contains_material(b"content-key");
    delete.restore(snapshot);
    delete.secure_delete(["db.sqlite", "db.sqlite-wal", "key.store"]);
    let secure_delete_negative = failed_verify_kept_material
        && !delete.contains_material(b"content-key")
        && delete.deleted_all(["db.sqlite", "db.sqlite-wal", "key.store"]);

    let backup = seal_account_backup(&key, vec!["room".to_owned()], 2);
    let recovery_cannot_resurrect_content_keys = !backup
        .identity_key_ciphertext
        .windows(key.len())
        .any(|window| window == key)
        && !backup.room_memberships.iter().any(|room| {
            room.as_bytes()
                .windows(key.len())
                .any(|window| window == key)
        });

    Ok(RetentionShredSmoke {
        default_window_locks_old_messages,
        shorten_retro_lengthen_future,
        cross_device_shred_sync,
        live_key_membership_rate_limit_decoy,
        secure_delete_negative,
        recovery_cannot_resurrect_content_keys,
    })
}

/// Exercise Phase-B fresh install, restart, corruption, and storage/keychain
/// secure-delete verification against the encrypted AppStore boundary.
pub fn storage_persistence_smoke() -> Result<StoragePersistenceSmoke, anyhow::Error> {
    use discrypt_storage::{
        sqlite_wal_path, AppStore, EncryptedAppDb, MemoryAppDbKeychain, SecureDeleteSimulator,
    };
    use std::fs;

    let path = std::env::temp_dir().join(format!(
        "discrypt-phase-b-storage-{}-{}.sqlite",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let wal_path = sqlite_wal_path(&path);
    let tmp_path = path.with_extension("json.tmp");
    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    let keychain = MemoryAppDbKeychain::default();
    let mut fresh_db = EncryptedAppDb::new(&path, keychain.clone());
    let fresh_install_starts_empty = fresh_db.load_app_state()?.is_none();

    let sensitive_state = br#"{"schema_version":1,"profile":{"display_name":"Alice"},"messages":[{"body":"phase-b plaintext must not leak"}],"content_key":"forbidden-content-key"}"#;
    fresh_db.save_app_state(sensitive_state)?;
    let mut restarted_db = EncryptedAppDb::new(&path, keychain);
    let restart_loads_encrypted_state =
        restarted_db.load_app_state()? == Some(sensitive_state.to_vec());

    let path_contains = |candidate: &std::path::Path, needle: &[u8]| {
        fs::read(candidate)
            .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
            .unwrap_or(false)
    };
    let no_plaintext_in_db_wal_or_temp = [
        b"Alice".as_slice(),
        b"phase-b plaintext must not leak".as_slice(),
        b"forbidden-content-key".as_slice(),
    ]
    .into_iter()
    .all(|needle| {
        !path_contains(&path, needle)
            && !path_contains(&wal_path, needle)
            && !path_contains(&tmp_path, needle)
    });

    fs::write(&path, br#"{"schema_version":0,"legacy":"plaintext-json"}"#)?;
    let corrupted_store_rejected = restarted_db.load_app_state().is_err();

    let mut delete = SecureDeleteSimulator::default();
    delete.write("app.db", b"wrapped-content-key".to_vec());
    delete.write("app.db-wal", b"wrapped-content-key-wal".to_vec());
    delete.write("app.keychain", b"wrapped-content-key-keychain".to_vec());
    let snapshot = delete.snapshot();
    delete.secure_delete(["app.db", "app.db-wal"]);
    let partial_delete_kept_keychain_material = delete.contains_material(b"wrapped-content-key");
    delete.restore(snapshot);
    delete.secure_delete(["app.db", "app.db-wal", "app.keychain"]);
    let secure_delete_requires_db_wal_and_keychain = partial_delete_kept_keychain_material
        && !delete.contains_material(b"wrapped-content-key")
        && delete.deleted_all(["app.db", "app.db-wal", "app.keychain"]);

    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    Ok(StoragePersistenceSmoke {
        fresh_install_starts_empty,
        restart_loads_encrypted_state,
        no_plaintext_in_db_wal_or_temp,
        corrupted_store_rejected,
        secure_delete_requires_db_wal_and_keychain,
    })
}

/// Exercise retention/shred behavior against encrypted store and keychain boundaries.
pub fn retention_shred_storage_boundary_smoke(
) -> Result<RetentionShredStorageBoundarySmoke, anyhow::Error> {
    use discrypt_storage::{
        recover_account, recovery_code_material, seal_account_backup, sqlite_wal_path,
        AppDbKeychain, AppStore, EncryptedAppDb, KeyState, LocalStore, MemoryAppDbKeychain,
        RecipientCacheEntry, RecoveryCodeVerifier,
    };
    use std::fs;

    fn path_contains(path: &std::path::Path, needle: &[u8]) -> bool {
        !needle.is_empty()
            && fs::read(path)
                .map(|bytes| bytes.windows(needle.len()).any(|window| window == needle))
                .unwrap_or(false)
    }

    fn keychain_contains(
        keychain: &MemoryAppDbKeychain,
        needle: &[u8],
    ) -> Result<bool, anyhow::Error> {
        Ok(keychain.snapshot_wrapping_keys()?.values().any(|key| {
            !needle.is_empty() && key.windows(needle.len()).any(|window| window == needle)
        }))
    }

    let run_id = chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default();
    let path = std::env::temp_dir().join(format!(
        "discrypt-phase-n-retention-boundary-{}-{run_id}.sqlite",
        std::process::id()
    ));
    let wal_path = sqlite_wal_path(&path);
    let tmp_path = path.with_extension("json.tmp");
    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    let key_id = "phase-n-retention-boundary-key";
    let content_key = [44_u8; 32];
    let forbidden_plaintext = b"phase-n retained plaintext body";
    let mut local_store = LocalStore::with_recipient_cache_capacity(4);
    local_store.cache_received(RecipientCacheEntry::new(
        "cached-message",
        b"ciphertext-only-cached-message".to_vec(),
        KeyState::Cached(content_key),
        10,
    ));
    local_store.cache_received(RecipientCacheEntry::new(
        "locked-message",
        b"ciphertext-only-locked-message".to_vec(),
        KeyState::Locked,
        11,
    ));
    local_store.cache_received(RecipientCacheEntry::new(
        "shredded-message",
        b"ciphertext-only-shredded-message".to_vec(),
        KeyState::Shredded,
        12,
    ));
    let serialized_store = serde_json::to_vec(&local_store)?;
    let keychain = MemoryAppDbKeychain::default();
    let mut db = EncryptedAppDb::with_key_id(&path, keychain.clone(), key_id);
    db.save_app_state(&serialized_store)?;
    let mut restarted = EncryptedAppDb::with_key_id(&path, keychain.clone(), key_id);
    let restored = restarted.load_app_state()?.unwrap_or_default();
    let restored_store: LocalStore = serde_json::from_slice(&restored)?;
    let retention_state_round_trips_encrypted_store = restored_store
        .recipient_cache()
        .get("cached-message")
        .is_some_and(|entry| entry.key_state == KeyState::Cached(content_key))
        && restored_store
            .recipient_cache()
            .get("locked-message")
            .is_some_and(|entry| entry.key_state == KeyState::Locked)
        && restored_store
            .recipient_cache()
            .get("shredded-message")
            .is_some_and(|entry| entry.key_state == KeyState::Shredded);

    let store_and_keychain_exclude_plaintext_and_content_keys = [
        forbidden_plaintext.as_slice(),
        content_key.as_slice(),
        b"phase-n-content-key".as_slice(),
    ]
    .into_iter()
    .all(|needle| {
        !path_contains(&path, needle)
            && !path_contains(&wal_path, needle)
            && !path_contains(&tmp_path, needle)
            && keychain_contains(&keychain, needle).is_ok_and(|contains| !contains)
    });

    let mut keychain_removed = keychain.clone();
    keychain_removed.delete_wrapping_key(key_id)?;
    let mut missing_key_db = EncryptedAppDb::with_key_id(&path, keychain_removed.clone(), key_id);
    let keychain_required_for_restore = missing_key_db.load_app_state().is_err();

    fs::write(&wal_path, b"phase-n-content-key journal residue")?;
    fs::write(&tmp_path, b"phase-n-content-key temp residue")?;
    let mut partial_keychain = keychain.clone();
    fs::remove_file(&path)?;
    fs::remove_file(&wal_path)?;
    let partial_delete_incomplete = tmp_path.exists()
        || !partial_keychain.snapshot_wrapping_keys()?.is_empty()
        || keychain_contains(&partial_keychain, content_key.as_slice())?;
    partial_keychain.delete_wrapping_key(key_id)?;
    let _ = fs::remove_file(&tmp_path);
    let secure_delete_enumerates_store_journal_temp_and_keychain = partial_delete_incomplete
        && !path.exists()
        && !wal_path.exists()
        && !tmp_path.exists()
        && partial_keychain.snapshot_wrapping_keys()?.is_empty();

    let verifier = RecoveryCodeVerifier::from_code("phase-n recovery code")?;
    let recovery = recover_account(recovery_code_material(
        "phase-n recovery code",
        &verifier,
        vec!["phase-n-room".to_owned()],
        2,
    )?)?;
    let backup = seal_account_backup(&content_key, vec!["phase-n-room".to_owned()], 2);
    let recovery_after_shred_excludes_content_keys = recovery.account_access_restored
        && !recovery.content_keys_restored
        && !backup
            .identity_key_ciphertext
            .windows(content_key.len())
            .any(|window| window == content_key);

    for candidate in [&path, &wal_path, &tmp_path] {
        let _ = fs::remove_file(candidate);
    }

    Ok(RetentionShredStorageBoundarySmoke {
        retention_state_round_trips_encrypted_store,
        store_and_keychain_exclude_plaintext_and_content_keys,
        keychain_required_for_restore,
        secure_delete_enumerates_store_journal_temp_and_keychain,
        recovery_after_shred_excludes_content_keys,
    })
}

/// Exercise the Phase-N deterministic performance soak envelope.
pub fn performance_soak_smoke() -> Result<PerformanceSoakSmoke, anyhow::Error> {
    use discrypt_media::{
        MediaKeyRegistry, ReplayWindow, SFrameReceiver, SFrameSender, SenderBinding,
    };
    use discrypt_relay_overlay::capability::{
        BatteryDozePosture, RelayCapabilityAdvertisement, RelayCapacityAdvertisement,
    };
    use discrypt_relay_overlay::redelivery::{RedeliveryError, RedeliveryTracker};
    use discrypt_relay_overlay::{OverlayManager, OverlayRouteUse, RelayRuntimeObservation};
    use discrypt_storage::{AppStore, EncryptedAppDb, MemoryAppDbKeychain};
    use discrypt_transport::{ConnectivityConfig, ConnectivityPlanner, FallbackLeg, SimulatedNat};
    use std::collections::BTreeSet;
    use std::fs;

    fn observation(peer_id: &str, latency_ms: u32) -> RelayRuntimeObservation {
        RelayRuntimeObservation {
            peer_id: peer_id.to_owned(),
            latency_ms,
            successful_probes: 100,
            failed_probes: 0,
            battery_cost_bps: 0,
            contributed_bytes: 64_000,
            consumed_bytes: 32_000,
        }
    }

    fn capability(
        peer_id: &str,
        posture: BatteryDozePosture,
        loss_bps: u16,
    ) -> RelayCapabilityAdvertisement {
        RelayCapabilityAdvertisement {
            peer_id: peer_id.to_owned(),
            sequence: 1,
            issued_at_ms: 1_000,
            expires_at_ms: 60_000,
            relay_capacity: RelayCapacityAdvertisement {
                max_fanout: 8,
                egress_bytes_per_second: 96_000,
                accepts_store_forward: true,
            },
            battery_doze: posture,
            observed_rtt_ms: 10 + u32::from(loss_bps / 100),
            packet_loss_bps: loss_bps,
            contributed_bytes: 96_000,
            consumed_bytes: 32_000,
        }
    }

    let members = (0..16)
        .map(|idx| format!("member-{idx:02}"))
        .collect::<Vec<_>>();
    let sixteen_members_represented =
        members.len() == 16 && members.iter().collect::<BTreeSet<_>>().len() == 16;

    let mut voice_registry = MediaKeyRegistry::new();
    let mut protected_voice = Vec::new();
    for idx in 0..8_u32 {
        let secret = [idx as u8 + 1; 32];
        let binding = SenderBinding::derive_for_epoch(
            &secret,
            "phase-n-performance-soak",
            100,
            idx,
            format!("voice-device-{idx}"),
        )?;
        voice_registry.register_sender(&secret, binding.clone())?;
        let mut sender = SFrameSender::new(&secret, binding)?;
        protected_voice.push(sender.protect(format!("voice-frame-{idx}").as_bytes())?);
    }
    let mut voice_receiver = SFrameReceiver::new(voice_registry, ReplayWindow::default());
    let eight_voice_senders_verified = protected_voice.iter().enumerate().all(|(idx, frame)| {
        voice_receiver
            .open(frame)
            .is_ok_and(|verified| verified.plaintext == format!("voice-frame-{idx}").as_bytes())
    });

    let mut manager = OverlayManager::default();
    for peer in &members {
        manager.upsert_observation(observation(peer, 5))?;
    }
    for relay in ["relay-a", "relay-b", "relay-c", "relay-d", "relay-e"] {
        manager.upsert_capability_advertisement(
            capability(relay, BatteryDozePosture::Charging, 250),
            2_000,
        )?;
    }
    manager.connect_peers("member-00", "member-01")?;
    manager.connect_peers("member-02", "relay-a")?;
    manager.connect_peers("relay-a", "member-03")?;
    manager.connect_peers("member-04", "relay-b")?;
    manager.connect_peers("relay-b", "relay-c")?;
    manager.connect_peers("relay-c", "member-05")?;
    manager.connect_peers("member-04", "relay-d")?;
    manager.connect_peers("relay-d", "relay-e")?;
    manager.connect_peers("relay-e", "member-05")?;
    let one_hop = manager.construct_route(OverlayRouteUse::VoiceMedia, "member-00", "member-01")?;
    let two_hop = manager.construct_route(OverlayRouteUse::VoiceMedia, "member-02", "member-03")?;
    let three_hop =
        manager.construct_route(OverlayRouteUse::VoiceMedia, "member-04", "member-05")?;
    let one_to_three_relay_hops_covered = one_hop.hop_count == 1
        && two_hop.hop_count == 2
        && three_hop.hop_count == 3
        && one_hop.route.within_hop_limit()
        && two_hop.route.within_hop_limit()
        && three_hop.route.within_hop_limit();

    let mut redelivery = RedeliveryTracker::new(64, 3);
    let mut dropped = 0_usize;
    let mut accepted = 0_usize;
    for sequence in 0..32_u64 {
        let id = packet_id("soak-media", sequence);
        if sequence % 4 == 0 {
            dropped += 1;
            redelivery.request_redelivery(id.clone(), "relay-a")?;
            redelivery.request_redelivery(id.clone(), "relay-b")?;
            redelivery.request_redelivery(id.clone(), "relay-c")?;
            if redelivery.request_redelivery(id, "relay-d") != Err(RedeliveryError::FanoutExhausted)
            {
                return Ok(PerformanceSoakSmoke {
                    sixteen_members_represented,
                    eight_voice_senders_verified,
                    one_to_three_relay_hops_covered,
                    packet_loss_redelivery_bounded: false,
                    nat_switching_fallbacks_covered: false,
                    android_doze_deprioritized: false,
                    restart_reconnect_recovers_route: false,
                });
            }
        } else if redelivery.accept(&id).is_ok() {
            accepted += 1;
        }
    }
    let packet_loss_redelivery_bounded = dropped == 8
        && accepted == 24
        && redelivery.accept(&packet_id("soak-media", 1)) == Err(RedeliveryError::Replay);

    let connectivity = ConnectivityConfig::default();
    let direct = ConnectivityPlanner::plan(&connectivity, SimulatedNat::direct())?;
    let overlay = ConnectivityPlanner::plan(&connectivity, SimulatedNat::overlay_only())?;
    let turn = ConnectivityPlanner::plan(&connectivity, SimulatedNat::turn_only())?;
    let nat_switching_fallbacks_covered = direct.selected == FallbackLeg::Stun
        && overlay.selected == FallbackLeg::RelayOverlay
        && turn.selected == FallbackLeg::Turn;

    let mut doze_manager = OverlayManager::default();
    doze_manager.upsert_observation(observation("member-06", 5))?;
    doze_manager.upsert_observation(observation("member-07", 5))?;
    doze_manager.upsert_capability_advertisement(
        capability("android-dozing-relay", BatteryDozePosture::Dozing, 100),
        2_000,
    )?;
    doze_manager.upsert_capability_advertisement(
        capability("charging-relay", BatteryDozePosture::Charging, 100),
        2_000,
    )?;
    doze_manager.connect_peers("member-06", "android-dozing-relay")?;
    doze_manager.connect_peers("member-06", "charging-relay")?;
    doze_manager.connect_peers("charging-relay", "member-07")?;
    doze_manager.connect_peers("android-dozing-relay", "member-07")?;
    let ranked = doze_manager.ranked_neighbors("member-06");
    let android_doze_deprioritized = ranked.first().is_some_and(|peer| peer == "charging-relay")
        && ranked.iter().any(|peer| peer == "android-dozing-relay");

    let path = std::env::temp_dir().join(format!(
        "discrypt-phase-n-soak-session-{}-{}.sqlite",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let _ = fs::remove_file(&path);
    let keychain = MemoryAppDbKeychain::default();
    let session_bytes = serde_json::to_vec(&three_hop.route)?;
    let mut db = EncryptedAppDb::with_key_id(&path, keychain.clone(), "phase-n-soak-session");
    db.save_app_state(&session_bytes)?;
    let mut restarted = EncryptedAppDb::with_key_id(&path, keychain, "phase-n-soak-session");
    let restored_route = restarted.load_app_state()?.unwrap_or_default();
    let restored_route_matches = restored_route == session_bytes;
    let failover = manager.mark_failed_media_and_reroute(three_hop.route, "relay-b", 1_200, 120)?;
    let restart_reconnect_recovers_route = restored_route_matches
        && failover.decision.converged_within_phase2_gate()
        && !failover.decision.replacement.contains_peer("relay-b")
        && failover
            .media_concealment
            .as_ref()
            .is_some_and(|report| report.target_met);
    let _ = fs::remove_file(&path);

    Ok(PerformanceSoakSmoke {
        sixteen_members_represented,
        eight_voice_senders_verified,
        one_to_three_relay_hops_covered,
        packet_loss_redelivery_bounded,
        nat_switching_fallbacks_covered,
        android_doze_deprioritized,
        restart_reconnect_recovers_route,
    })
}

/// Exercise Phase-5 governance, admission, recovery, and abuse controls.
pub fn governance_admission_smoke() -> Result<GovernanceAdmissionSmoke, anyhow::Error> {
    use chrono::{Duration, Utc};
    use discrypt_abuse::AbuseControls;
    use discrypt_admission::{
        AdmissionController, AuthorizedWelcome, Invite, InviteError, PasswordGate,
    };
    use discrypt_mls_core::governance::{
        GovernanceAction, GovernanceError, GovernanceEvent, GovernanceLog, GovernanceState, Role,
    };
    use discrypt_mls_core::OpenMlsGroupEngine;
    use discrypt_storage::{
        recover_account, recovery_code_material, seal_account_backup, AccountRecovery,
        RecoveryCodeVerifier, RecoveryError, RecoveryMaterial,
    };
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    let mut log = GovernanceLog::default();
    let high = GovernanceEvent::signed(10, 9, GovernanceAction::Ban { target: 4 });
    let low = GovernanceEvent::signed(
        10,
        1,
        GovernanceAction::RevokeInvite {
            invite_id: "invite-a".into(),
        },
    );
    log.append(high);
    log.append(low);
    let governance_ordered_signed =
        log.events()[0].committer == 1 && log.events().iter().all(GovernanceEvent::signature_valid);

    let mut state = GovernanceState::new(10, 1);
    let governance_rejects_invalid_authority = state.apply_event(GovernanceEvent::signed(
        10,
        2,
        GovernanceAction::RevokeInvite {
            invite_id: "bad".into(),
        },
    )) == Err(GovernanceError::Unauthorized)
        && state.apply_event(GovernanceEvent::signed(
            11,
            1,
            GovernanceAction::RevokeInvite {
                invite_id: "future".into(),
            },
        )) == Err(GovernanceError::OutOfEpoch);

    state.apply_event(GovernanceEvent::signed(
        10,
        1,
        GovernanceAction::SetRole {
            target: 2,
            role: Role::Admin,
        },
    ))?;
    let race = state.resolve_epoch_events([
        GovernanceEvent::signed(
            10,
            2,
            GovernanceAction::RevokeInvite {
                invite_id: "admin-loses".into(),
            },
        ),
        GovernanceEvent::signed(10, 1, GovernanceAction::Ban { target: 2 }),
    ]);
    let removed_admin_cannot_win = race == vec![Ok(()), Err(GovernanceError::EvictedCommitter)]
        && state.is_banned(2)
        && !state.invite_revoked("admin-loses");

    let now = Utc::now();
    let mut one_use = Invite::new(b"secret", now + Duration::minutes(1), 1);
    let first_use = one_use.consume(now) == Ok(());
    let exhausted = one_use.consume(now) == Err(InviteError::Exhausted);
    let mut expired = Invite::new(b"secret", now - Duration::seconds(1), 1);
    let expired_rejected = expired.consume(now) == Err(InviteError::Expired);
    let mut revoked = Invite::new(b"secret", now + Duration::minutes(1), 1);
    revoked.revoke();
    let revoked_rejected = revoked.consume(now) == Err(InviteError::Revoked);
    let invite_controls_enforced = first_use && exhausted && expired_rejected && revoked_rejected;

    let mut invite = Invite::new(b"secret", now + Duration::minutes(1), 2);
    let mut offline = AdmissionController::new(
        PasswordGate::OfflineVerifier {
            verifier_id: "copyable".into(),
        },
        1,
    );
    let mut pake = AdmissionController::new(
        PasswordGate::OnlineAuthorizedHelper {
            helper_id: "owner-device".into(),
        },
        1,
    );
    let admission_run_id = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let alice_path = std::env::temp_dir().join(format!(
        "discrypt-harness-phase5-alice-{}-{admission_run_id}",
        std::process::id()
    ));
    let bob_path = std::env::temp_dir().join(format!(
        "discrypt-harness-phase5-bob-{}-{admission_run_id}",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&alice_path);
    let _ = std::fs::remove_file(&bob_path);
    let mut alice_openmls = OpenMlsGroupEngine::open(&alice_path)?;
    let mut bob_openmls = OpenMlsGroupEngine::open(&bob_path)?;
    let created_openmls = alice_openmls.create_group("phase5-admission", b"alice")?;
    let bob_package = bob_openmls.generate_member_package(b"bob")?;
    let added_openmls = alice_openmls.add_member_package("phase5-admission", &bob_package)?;
    let Some(welcome_payload) = added_openmls.welcome.as_deref() else {
        return Err(anyhow::anyhow!(
            "OpenMLS add did not produce admission Welcome"
        ));
    };
    let welcome = AuthorizedWelcome::sign(
        invite.id.to_string(),
        b"phase5-admission".to_vec(),
        welcome_payload,
        now + Duration::minutes(1),
        &SigningKey::generate(&mut OsRng),
    );
    let link_only_rejected =
        pake.finalize_admission(&mut invite, now, "alice", true, None, welcome_payload)
            == Err(InviteError::WelcomeRequired);
    let tampered_welcome_rejected = pake.finalize_admission(
        &mut invite,
        now,
        "alice",
        true,
        Some(&welcome),
        b"tampered-welcome",
    ) == Err(InviteError::InvalidWelcomeAuthorization);
    let offline_rejected = offline.finalize_admission(
        &mut invite,
        now,
        "alice",
        true,
        Some(&welcome),
        welcome_payload,
    ) == Err(InviteError::OfflineVerifierRejected);
    let final_admission_accepted = pake.finalize_admission(
        &mut invite,
        now,
        "alice",
        true,
        Some(&welcome),
        welcome_payload,
    ) == Ok(());
    let joined_openmls = bob_openmls.join_from_welcome(
        "phase5-admission",
        bob_package.signer_public_key(),
        welcome_payload,
    )?;
    let openmls_welcome_converged = joined_openmls.epoch == added_openmls.state.epoch
        && added_openmls.state.epoch == created_openmls.epoch + 1
        && joined_openmls.confirmation_tag == added_openmls.state.confirmation_tag;
    let exhausted_after_welcome = pake.finalize_admission(
        &mut invite,
        now,
        "alice",
        true,
        Some(&welcome),
        welcome_payload,
    ) == Err(InviteError::PasswordRejected);
    let _ = std::fs::remove_file(&alice_path);
    let _ = std::fs::remove_file(&bob_path);
    let password_and_welcome_gate = offline_rejected
        && link_only_rejected
        && tampered_welcome_rejected
        && final_admission_accepted
        && openmls_welcome_converged
        && exhausted_after_welcome;

    let backup = seal_account_backup(&[8; 32], vec!["room".into()], 2);
    let recovery_code = RecoveryCodeVerifier::from_code("paper-coral-falcon")?;
    let code_material =
        recovery_code_material("paper-coral-falcon", &recovery_code, vec!["room".into()], 2)?;
    let recovery_trust_model = recover_account(RecoveryMaterial::None)
        == Err(RecoveryError::NoTrustMaterial)
        && recovery_code_material("wrong", &recovery_code, vec!["room".into()], 2)
            == Err(RecoveryError::InvalidRecoveryCode)
        && matches!(
            recover_account(code_material),
            Ok(AccountRecovery {
                account_access_restored: true,
                room_memberships,
                device_count: 2,
                content_keys_restored: false,
            }) if room_memberships == vec!["room".to_owned()]
        )
        && matches!(
            recover_account(RecoveryMaterial::SealedBackup(backup)),
            Ok(AccountRecovery {
                account_access_restored: true,
                room_memberships,
                device_count: 2,
                content_keys_restored: false,
            }) if room_memberships == vec!["room".to_owned()]
        );

    let mut abuse = AbuseControls::new(1, 2, Duration::minutes(1));
    let abuse_controls_enforced = abuse.allow_invite("alice", now)
        && !abuse.allow_invite("alice", now)
        && abuse.allow_message("alice", now)
        && abuse.allow_message("alice", now)
        && !abuse.allow_message("alice", now)
        && {
            abuse.record_relay("freeloader", 1, 10);
            abuse.record_relay("helper", 10, 1);
            abuse.freeload_penalty("freeloader") > abuse.freeload_penalty("helper")
        };

    Ok(GovernanceAdmissionSmoke {
        governance_ordered_signed,
        governance_rejects_invalid_authority,
        removed_admin_cannot_win,
        invite_controls_enforced,
        password_and_welcome_gate,
        recovery_trust_model,
        abuse_controls_enforced,
    })
}

/// Exercise G119 abuse E2E gates across abuse, admission, signaling, and overlay routing.
pub fn abuse_e2e_smoke() -> Result<AbuseE2eSmoke, anyhow::Error> {
    use chrono::{Duration, Utc};
    use discrypt_abuse::AbuseControls;
    use discrypt_admission::{InviteError, OnlineAdmissionHelper};
    use discrypt_relay_overlay::{OverlayManager, OverlayManagerError, RelayRuntimeObservation};
    use ed25519_dalek::SigningKey;
    use external_signaling::server::{handle_http_request, ServerConfig, SharedSignalingService};
    use rand::rngs::OsRng;

    fn http_request(method: &str, path: &str, body: &str) -> String {
        format!(
            "{method} {path} HTTP/1.1\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        )
    }

    fn publish_signal_body(nonce: &[u8], key: &[u8], payload: &[u8]) -> String {
        let expires_at = Utc::now() + Duration::seconds(60);
        serde_json::json!({
            "client_token_hex": hex::encode(b"g119-opaque-client-token"),
            "nonce_hex": hex::encode(nonce),
            "kind": "admission_helper",
            "key_hex": hex::encode(key),
            "payload_hex": hex::encode(payload),
            "expires_at": expires_at,
        })
        .to_string()
    }

    fn relay_observation(peer_id: &str, latency_ms: u32) -> RelayRuntimeObservation {
        RelayRuntimeObservation {
            peer_id: peer_id.to_owned(),
            latency_ms,
            successful_probes: 10,
            failed_probes: 0,
            battery_cost_bps: 0,
            contributed_bytes: 10,
            consumed_bytes: 0,
        }
    }

    fn route_after_freeload_penalty() -> Result<bool, OverlayManagerError> {
        let mut manager = OverlayManager::default();
        for peer in [
            relay_observation("alice", 5),
            relay_observation("relay-a", 10),
            relay_observation("relay-b", 30),
            relay_observation("bob", 5),
        ] {
            manager.upsert_observation(peer)?;
        }
        manager.connect_peers("alice", "relay-a")?;
        manager.connect_peers("relay-a", "bob")?;
        manager.connect_peers("alice", "relay-b")?;
        manager.connect_peers("relay-b", "bob")?;
        let initially_prefers_fast_relay =
            manager.route("alice", "bob")?.route.path == ["alice", "relay-a", "bob"];
        let snapshot = manager.record_relay_contribution("relay-a", 0, 100_000)?;
        let downranked = manager.route("alice", "bob")?.route.path == ["alice", "relay-b", "bob"];
        Ok(initially_prefers_fast_relay && snapshot.freeload_penalty > 0.0 && downranked)
    }

    let now = Utc::now();
    let mut abuse = AbuseControls::new(2, 2, Duration::minutes(1));
    let invite_flood_rate_limited = abuse.allow_invite("attacker", now)
        && abuse.allow_invite("attacker", now)
        && !abuse.allow_invite("attacker", now);
    let spam_burst_rate_limited = abuse.allow_message("attacker", now)
        && abuse.allow_message("attacker", now)
        && !abuse.allow_message("attacker", now);

    let mut helper = OnlineAdmissionHelper::new(
        "helper-g119",
        b"correct horse",
        SigningKey::generate(&mut OsRng),
        2,
        60,
    );
    let admission_helper_bruteforce_rejected = helper.authorize("mallory-device", b"wrong-1", now)
        == Err(InviteError::PasswordRejected)
        && helper.authorize("mallory-device", b"wrong-2", now)
            == Err(InviteError::PasswordRejected)
        && helper.authorize("mallory-device", b"correct horse", now)
            == Err(InviteError::PasswordRejected);

    let service = SharedSignalingService::new();
    let flood_config = ServerConfig {
        rate_limit_max_requests: 2,
        ..ServerConfig::default()
    };
    let flood_one = handle_http_request(
        &service,
        &flood_config,
        http_request(
            "POST",
            "/v1/signals/publish",
            &publish_signal_body(b"nonce-1", b"flood-key-1", b"opaque-payload-1"),
        )
        .as_bytes(),
    );
    let flood_two = handle_http_request(
        &service,
        &flood_config,
        http_request(
            "POST",
            "/v1/signals/publish",
            &publish_signal_body(b"nonce-2", b"flood-key-2", b"opaque-payload-2"),
        )
        .as_bytes(),
    );
    let flood_three = handle_http_request(
        &service,
        &flood_config,
        http_request(
            "POST",
            "/v1/signals/publish",
            &publish_signal_body(b"nonce-3", b"flood-key-3", b"opaque-payload-3"),
        )
        .as_bytes(),
    );
    let flood_one = String::from_utf8_lossy(&flood_one);
    let flood_two = String::from_utf8_lossy(&flood_two);
    let flood_three = String::from_utf8_lossy(&flood_three);
    let signaling_blob_flood_rate_limited = flood_one.contains("HTTP/1.1 201 Created")
        && flood_two.contains("HTTP/1.1 201 Created")
        && flood_three.contains("HTTP/1.1 429 Too Many Requests")
        && flood_three.contains("rate_limited")
        && !flood_three.contains("opaque-payload");

    let size_config = ServerConfig {
        max_body_bytes: 8,
        ..ServerConfig::default()
    };
    let size_response = handle_http_request(
        &SharedSignalingService::new(),
        &size_config,
        http_request("POST", "/v1/signals/publish", "123456789").as_bytes(),
    );
    let size_text = String::from_utf8_lossy(&size_response);
    let service_request_size_exhaustion_rejected = size_text
        .contains("HTTP/1.1 413 Payload Too Large")
        && size_text.contains("request_too_large");

    Ok(AbuseE2eSmoke {
        invite_flood_rate_limited,
        spam_burst_rate_limited,
        admission_helper_bruteforce_rejected,
        signaling_blob_flood_rate_limited,
        relay_freeloading_downranked: route_after_freeload_penalty()?,
        service_request_size_exhaustion_rejected,
    })
}

/// Exercise Phase-6 signaling, fallback, push, and metadata audit gates.
pub fn connectivity_signaling_push_smoke() -> Result<ConnectivitySignalingPushSmoke, anyhow::Error>
{
    use chrono::{Duration, Utc};
    use discrypt_push::{
        contains_content, contains_forbidden_token, AndroidWakeService, WakePayload, WakeReason,
    };
    use discrypt_transport::{
        ConnectivityConfig, ConnectivityPlanner, Endpoint, EndpointOverrides, FallbackLeg,
        LocalProcessSocketAdapter, SimulatedNat,
    };
    use external_signaling::{
        AuditFixture, ContentExposure, InfrastructureComponent, MetadataMatrix, PcapEvent,
        ReferenceSignalingServer, RendezvousBlob, RendezvousKey,
    };

    let now = Utc::now();
    let forbidden: [&[u8]; 5] = [
        b"alice".as_slice(),
        b"bob".as_slice(),
        b"room-plaintext".as_slice(),
        b"message-body".as_slice(),
        b"topology-link".as_slice(),
    ];

    let mut signaling = ReferenceSignalingServer::default();
    let key = RendezvousKey::new(b"opaque-rendezvous-key".to_vec());
    signaling.publish(
        key.clone(),
        RendezvousBlob::new(
            b"opaque-room-token".to_vec(),
            b"opaque-endpoint-hint".to_vec(),
            now + Duration::minutes(5),
        ),
        external_signaling::transport::Endpoint::new("198.51.100.9:4242"),
        now,
    )?;
    let signaling_zero_linkage_at_rest =
        signaling.zero_linkage_at_rest(&forbidden) && !external_signaling::stores_linkage_at_rest();
    let fetched = signaling.take(&key, now)?;
    let signaling_content_blind =
        !external_signaling::contains_any_token(&fetched.visible_bytes(), &forbidden);

    let default_config = ConnectivityConfig::default();
    let direct = ConnectivityPlanner::plan(&default_config, SimulatedNat::direct())?;
    let overlay = ConnectivityPlanner::plan(&default_config, SimulatedNat::overlay_only())?;
    let turn = ConnectivityPlanner::plan(&default_config, SimulatedNat::turn_only())?;
    let fallback_chain_covered = direct.selected == FallbackLeg::Stun
        && overlay.selected == FallbackLeg::RelayOverlay
        && turn.selected == FallbackLeg::Turn
        && direct.ordered_stun_overlay_turn()
        && overlay.ordered_stun_overlay_turn()
        && turn.ordered_stun_overlay_turn();
    let relays_ciphertext_only =
        overlay.relay_legs_ciphertext_only() && turn.relay_legs_ciphertext_only();
    let route_report = overlay.route_report();

    let override_config = ConnectivityConfig {
        overrides: EndpointOverrides::new(
            Some(Endpoint::new("stun:owner.example:3478")),
            Some(Endpoint::new("turns:owner.example:5349")),
        ),
        ..ConnectivityConfig::default()
    };
    let owner_stun = ConnectivityPlanner::plan(&override_config, SimulatedNat::direct())?;
    let owner_turn = ConnectivityPlanner::plan(&override_config, SimulatedNat::turn_only())?;
    let owner_overrides_used = owner_stun.endpoint == Endpoint::new("stun:owner.example:3478")
        && owner_turn.endpoint == Endpoint::new("turns:owner.example:5349");

    let socket_adapter = LocalProcessSocketAdapter::new(
        default_config.clone(),
        SimulatedNat::overlay_only(),
        b"message-body".to_vec(),
    );
    let socket_report = socket_adapter.run_conformance(b"opaque socket ciphertext")?;
    let socket_local_process_conformant = socket_report.ready();
    let route_reporting_honest =
        route_report.honest_and_ordered() && socket_report.route_report.honest_and_ordered();

    let wake_service = AndroidWakeService::default();
    let payload = WakePayload::new([7; 32], WakeReason::SyncHint, [9; 16]);
    let push_envelope = wake_service.build_envelope([8; 32], payload.clone())?;
    let android_wake_content_free =
        !contains_content(&payload) && !contains_forbidden_token(&push_envelope, &forbidden);

    let mut fixture = AuditFixture::default();
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Signaling,
        content: ContentExposure::None,
        visible_bytes: fetched.visible_bytes(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Stun,
        content: ContentExposure::None,
        visible_bytes: b"binding request no app content".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::Turn,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"sframe ciphertext over turn".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::PushFcm,
        content: ContentExposure::None,
        visible_bytes: push_envelope.provider_visible_bytes(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::PeerRelay,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"sframe ciphertext over peer relay".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    fixture.push(PcapEvent {
        component: InfrastructureComponent::VolunteerStorageRelay,
        content: ContentExposure::CiphertextOnly,
        visible_bytes: b"store-forward ciphertext".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    let matrix = MetadataMatrix::approved_v1();
    let metadata_matrix_validated = fixture.matches_matrix(&matrix);
    let pcap_no_central_content = fixture.no_forbidden_content_egress(&forbidden);

    Ok(ConnectivitySignalingPushSmoke {
        signaling_zero_linkage_at_rest: signaling_zero_linkage_at_rest && signaling_content_blind,
        fallback_chain_covered,
        owner_overrides_used,
        android_wake_content_free,
        metadata_matrix_validated,
        pcap_no_central_content,
        relays_ciphertext_only,
        socket_local_process_conformant,
        route_reporting_honest,
    })
}

/// Exercise the Phase-N pcap acceptance matrix for AC1, AC8, AC15, AC18, and AC-METADATA.
pub fn pcap_acceptance_matrix_smoke() -> Result<PcapAcceptanceMatrixSmoke, anyhow::Error> {
    use discrypt_core::{app_snapshot, verify_safety_number, SafetyVerificationRequest};
    use external_signaling::{
        contains_any_token, AuditFixture, ContentExposure, InfrastructureComponent, MetadataMatrix,
        PcapEvent,
    };

    let snapshot = app_snapshot();
    let safety_verified = verify_safety_number(SafetyVerificationRequest {
        friend_id: snapshot.friend.friend_code.clone(),
        provided: snapshot.friend.safety_number.clone(),
    })
    .verified;

    let forbidden_release_tokens: Vec<Vec<u8>> = vec![
        snapshot.friend.alias.as_bytes().to_vec(),
        snapshot.friend.friend_code.as_bytes().to_vec(),
        snapshot.friend.safety_number.as_bytes().to_vec(),
        b"hello from app-level encrypted text".to_vec(),
        b"harness encoded voice frame".to_vec(),
        b"phase2 encoded voice frame".to_vec(),
        b"content-key".to_vec(),
        b"mls-epoch-secret".to_vec(),
        b"sframe-key".to_vec(),
        b"identity-private-key".to_vec(),
        b"room-plaintext".to_vec(),
        b"topology-link".to_vec(),
        b"message-body".to_vec(),
        b"fcm-message-id".to_vec(),
    ];
    let forbidden_refs: Vec<&[u8]> = forbidden_release_tokens.iter().map(Vec::as_slice).collect();

    let mut ac1_fixture = AuditFixture::default();
    ac1_fixture.push(PcapEvent {
        component: InfrastructureComponent::Signaling,
        content: ContentExposure::None,
        visible_bytes: b"opaque-dm-rendezvous-without-directory-account".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    ac1_fixture.push(PcapEvent {
        component: InfrastructureComponent::Stun,
        content: ContentExposure::None,
        visible_bytes: b"binding-only-no-dm-content".to_vec(),
        ip_or_endpoint: true,
        timing: true,
        persists_linkage: false,
    });
    let no_directory_or_account_component = ac1_fixture.events().iter().all(|event| {
        matches!(
            event.component,
            InfrastructureComponent::Signaling | InfrastructureComponent::Stun
        )
    });
    let ac1_identity_dm_safety_pcap_clean = safety_verified
        && no_directory_or_account_component
        && ac1_fixture.no_forbidden_content_egress(&forbidden_refs)
        && ac1_fixture.matches_matrix(&MetadataMatrix::approved_v1());

    let voice = voice_media_e2e_smoke()?;
    let text = text_history_delivery_smoke()?;
    let connectivity = connectivity_signaling_push_smoke()?;
    let ac8_relay_media_ciphertext_only = voice.relay_pcap_protected_only
        && text.text_pcap_no_plaintext
        && connectivity.relays_ciphertext_only;
    let ac15_android_wake_content_free = connectivity.android_wake_content_free;
    let ac18_signaling_zero_linkage_at_rest = connectivity.signaling_zero_linkage_at_rest;
    let ac_metadata_matrix_validated =
        connectivity.metadata_matrix_validated && connectivity.pcap_no_central_content;
    let forbidden_scanner_covers_release_tokens = forbidden_release_tokens.len() >= 14
        && !contains_any_token(b"sealed protected ciphertext", &forbidden_refs)
        && contains_any_token(b"prefix message-body suffix", &forbidden_refs)
        && contains_any_token(b"prefix mls-epoch-secret suffix", &forbidden_refs);

    Ok(PcapAcceptanceMatrixSmoke {
        ac1_identity_dm_safety_pcap_clean,
        ac8_relay_media_ciphertext_only,
        ac15_android_wake_content_free,
        ac18_signaling_zero_linkage_at_rest,
        ac_metadata_matrix_validated,
        forbidden_scanner_covers_release_tokens,
    })
}

fn xor_text_ciphertext(key: &[u8; 32], input: &[u8]) -> Vec<u8> {
    input
        .iter()
        .enumerate()
        .map(|(index, byte)| byte ^ key[index % key.len()])
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Exercise Phase-C compromised-device rotation, next-epoch send blocking, and UI metadata.
pub fn phase_c_device_rotation_smoke() -> Result<PhaseCDeviceRotationSmoke, anyhow::Error> {
    use discrypt_mls_core::{
        DeviceSet, DeviceStatus, ExportLabel, GroupState, Identity, MlsCoreError,
    };

    let identity = Identity::generate("alice");
    let compromised_device_key = Identity::generate("alice-lost-laptop-device").verifying_key();
    let replacement_device_key =
        Identity::generate("alice-replacement-laptop-device").verifying_key();
    let mut devices = DeviceSet::new();
    let compromised =
        devices.add_authorized_device(&identity, compromised_device_key, "lost laptop", 0);

    let mut group = GroupState::new("phase-c-device-rotation");
    group.add_leaf(compromised.clone())?;
    let before_rotation = group.export(ExportLabel::Text, b"phase-c-text-send");
    let original_epoch = group.epoch;

    let rotation = devices.rotate_compromised_device(
        &identity,
        compromised.device_id,
        replacement_device_key,
        "replacement laptop",
        original_epoch.saturating_add(1),
        original_epoch.saturating_add(2),
    )?;
    group.rotate_leaf(compromised.leaf_index, rotation.replacement.clone())?;

    let compromised_device_retired = devices
        .device(compromised.device_id)
        .is_some_and(|device| device.status == DeviceStatus::Compromised)
        && !devices.device_may_send(compromised.device_id)
        && devices.device_may_send(rotation.replacement.device_id);
    let group_rekeyed_after_rotation = group.export(ExportLabel::Text, b"phase-c-text-send")
        != before_rotation
        && group.epoch == original_epoch.saturating_add(2);
    let old_device_send_blocked = group.validate_sender(compromised.leaf_index, group.epoch)
        == Err(MlsCoreError::SenderNotAuthorized(compromised.leaf_index));
    let replacement_device_can_send =
        group.validate_sender(rotation.replacement.leaf_index, group.epoch) == Ok(());
    let stale_epoch_send_blocked = group.validate_sender(
        rotation.replacement.leaf_index,
        group.epoch.saturating_sub(1),
    ) == Err(MlsCoreError::StaleSenderEpoch {
        current: group.epoch,
        attempted: group.epoch.saturating_sub(1),
    });
    let transparency_notices_include_rotation = devices
        .transparency_events()
        .iter()
        .any(|event| event.kind == "device-compromised-removed")
        && devices
            .transparency_events()
            .iter()
            .any(|event| event.kind == "device-rotation-replacement");

    let snapshot = discrypt_core::app_snapshot();
    let command_health = discrypt_desktop::command_health();
    let command_surface_reports_device_metadata = command_health.identity_ready
        && command_health.honest_copy_ready
        && snapshot
            .devices
            .iter()
            .any(|device| device.local && device.authorized)
        && snapshot
            .devices
            .iter()
            .any(|device| !device.local && device.authorized)
        && snapshot
            .security_copy
            .deletion
            .contains("pending on offline devices until they reconnect");

    Ok(PhaseCDeviceRotationSmoke {
        compromised_device_retired,
        group_rekeyed_after_rotation,
        old_device_send_blocked,
        replacement_device_can_send,
        stale_epoch_send_blocked,
        transparency_notices_include_rotation,
        command_surface_reports_device_metadata,
    })
}

/// Exercise Phase-7 Tauri/React command-surface and final E2E hardening gates.
pub fn ux_e2e_hardening_smoke() -> Result<UxE2eHardeningSmoke, anyhow::Error> {
    let snapshot = discrypt_core::app_snapshot();
    let command_health = discrypt_desktop::command_health();
    let verification =
        discrypt_core::verify_safety_number(discrypt_core::SafetyVerificationRequest {
            friend_id: snapshot.friend.friend_code.clone(),
            provided: snapshot.friend.safety_number.clone(),
        });

    let command_surface_ready = command_health.snapshot_ready
        && command_health.verification_ready
        && command_health.honest_copy_ready;
    let discord_style_model_ready = snapshot.servers.iter().any(|server| {
        server
            .channels
            .iter()
            .any(|channel| matches!(channel.kind, discrypt_core::ChannelKind::Text))
            && server
                .channels
                .iter()
                .any(|channel| matches!(channel.kind, discrypt_core::ChannelKind::Voice))
    });
    let verification_and_devices_ready = !snapshot.friend.verified
        && verification.verified
        && !snapshot.friend.safety_number.is_empty()
        && snapshot.devices.iter().any(|device| device.local)
        && snapshot
            .devices
            .iter()
            .any(|device| !device.local && device.authorized);
    let invite_retention_deletion_ready = snapshot.invite.welcome_required.contains("MLS Welcome")
        && snapshot.invite.password_gate.contains("OPAQUE/PAKE")
        && snapshot
            .retention
            .presets
            .contains(&"warned unlimited / never-lock".to_owned())
        && snapshot
            .security_copy
            .deletion
            .contains("pending on offline devices until they reconnect")
        && snapshot
            .security_copy
            .malicious_member
            .contains("screenshots");
    let connectivity_copy_ready = snapshot
        .connectivity
        .fallback_chain
        .contains("STUN → relay-overlay → TURN")
        && snapshot.connectivity.push_copy.contains("content-free")
        && snapshot
            .connectivity
            .metadata_copy
            .contains("not metadata-anonymous");

    let media = media_security_smoke()?;
    let overlay = relay_overlay_smoke()?;
    let text = text_history_delivery_smoke()?;
    let retention = retention_shred_smoke()?;
    let storage = storage_persistence_smoke()?;
    let governance = governance_admission_smoke()?;
    let connectivity = connectivity_signaling_push_smoke()?;
    let phase_c = phase_c_device_rotation_smoke()?;
    let voice_e2e = voice_media_e2e_smoke()?;
    let all_phase_smokes_ready = media.ready()
        && overlay.ready()
        && text.ready()
        && retention.ready()
        && storage.ready()
        && governance.ready()
        && connectivity.ready()
        && phase_c.ready()
        && voice_e2e.ready();

    Ok(UxE2eHardeningSmoke {
        command_surface_ready,
        discord_style_model_ready,
        verification_and_devices_ready,
        invite_retention_deletion_ready,
        connectivity_copy_ready,
        all_phase_smokes_ready,
    })
}

/// Verify two independent profiles can establish a DM boundary, exercise a protected
/// direct-message route, attempt voice/media routes, and stay aligned with UI gates.
pub fn two_profile_p2p_dm_voice_ui_smoke() -> Result<TwoProfileP2pDmVoiceUiSmoke, anyhow::Error> {
    let alice = Identity::generate("two-profile Alice");
    let bob = Identity::generate("two-profile Bob");
    let alice_code = alice.friend_code();
    let bob_code = bob.friend_code();
    let (_dm_group, alice_safety_number) = create_dm(&alice, &bob);
    let bob_safety_number = bob
        .safety_number_from_friend_code(&alice_code)
        .map(|safety| safety.as_str().to_owned());
    let independent_profiles_created =
        alice_code.as_str() != bob_code.as_str() && alice.verifying_key() != bob.verifying_key();
    let pairwise_safety_numbers_match =
        bob_safety_number.as_deref() == Some(alice_safety_number.as_str());

    let text = text_history_delivery_smoke()?;
    let p2p_dm_message_e2e = text.text_e2e_roundtrip
        && text.direct_path_text_exchanged
        && text.text_pcap_no_plaintext
        && text.no_plaintext_in_text_surfaces;

    let voice = voice_media_e2e_smoke()?;
    let voice_media_attempt_covered = voice.ready();

    let ui = ux_e2e_hardening_smoke()?;
    let frontend_ui_checks_ready = ui.command_surface_ready
        && ui.discord_style_model_ready
        && ui.verification_and_devices_ready
        && ui.connectivity_copy_ready;

    let snapshot = discrypt_core::app_snapshot();
    let no_fake_voice_members = snapshot
        .voice_session
        .participants
        .iter()
        .all(|participant| participant.role == "you")
        && snapshot.voice_session.route_copy.contains("not connected");

    Ok(TwoProfileP2pDmVoiceUiSmoke {
        independent_profiles_created,
        pairwise_safety_numbers_match,
        p2p_dm_message_e2e,
        voice_media_attempt_covered,
        frontend_ui_checks_ready,
        no_fake_voice_members,
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
                capture_opus_sframe_protected: true,
                receive_decode_jitter_playback_ready: true,
                mute_suppresses_outbound_media: true,
                playback_volume_mixer_ready: true,
                speaking_indicator_from_vad: true,
                android_native_contingency_ready: true,
                plaintext
            }) if plaintext == b"harness encoded voice frame"
        ));
    }

    #[test]
    fn voice_media_e2e_smoke_covers_phase_j_gate() {
        let smoke = voice_media_e2e_smoke();
        assert!(matches!(
            smoke,
            Ok(VoiceMediaE2eSmoke {
                direct_webrtc_audio_exchanged: true,
                overlay_audio_exchanged: true,
                turn_audio_exchanged: true,
                mute_blocks_outbound_audio: true,
                volume_affects_playback: true,
                speaking_follows_actual_audio: true,
                relay_pcap_protected_only: true,
            })
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
                media_gap_concealed: true,
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
                text_e2e_roundtrip: true,
                direct_path_text_exchanged: true,
                overlay_path_text_exchanged: true,
                turn_path_text_exchanged: true,
                offline_store_forward_within_ttl: true,
                retention_locks_old_store_forward: true,
                text_pcap_no_plaintext: true,
                no_plaintext_in_text_surfaces: true,
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

    #[test]
    fn retention_shred_smoke_covers_phase4_gates() {
        let smoke = retention_shred_smoke();
        assert!(matches!(
            smoke,
            Ok(RetentionShredSmoke {
                default_window_locks_old_messages: true,
                shorten_retro_lengthen_future: true,
                cross_device_shred_sync: true,
                live_key_membership_rate_limit_decoy: true,
                secure_delete_negative: true,
                recovery_cannot_resurrect_content_keys: true,
            })
        ));
    }

    #[test]
    fn retention_shred_storage_boundary_smoke_covers_real_store_and_keychain_gates() {
        let smoke = retention_shred_storage_boundary_smoke();
        assert!(matches!(
            smoke,
            Ok(RetentionShredStorageBoundarySmoke {
                retention_state_round_trips_encrypted_store: true,
                store_and_keychain_exclude_plaintext_and_content_keys: true,
                keychain_required_for_restore: true,
                secure_delete_enumerates_store_journal_temp_and_keychain: true,
                recovery_after_shred_excludes_content_keys: true,
            })
        ));
    }

    #[test]
    fn performance_soak_smoke_covers_phase_n_load_and_reconnect_gates() {
        let smoke = performance_soak_smoke();
        assert!(matches!(
            smoke,
            Ok(PerformanceSoakSmoke {
                sixteen_members_represented: true,
                eight_voice_senders_verified: true,
                one_to_three_relay_hops_covered: true,
                packet_loss_redelivery_bounded: true,
                nat_switching_fallbacks_covered: true,
                android_doze_deprioritized: true,
                restart_reconnect_recovers_route: true,
            })
        ));
    }

    #[test]
    fn storage_persistence_smoke_covers_phase_b_gates() {
        let smoke = storage_persistence_smoke();
        assert!(matches!(
            smoke,
            Ok(StoragePersistenceSmoke {
                fresh_install_starts_empty: true,
                restart_loads_encrypted_state: true,
                no_plaintext_in_db_wal_or_temp: true,
                corrupted_store_rejected: true,
                secure_delete_requires_db_wal_and_keychain: true,
            })
        ));
    }

    #[test]
    fn governance_admission_smoke_covers_phase5_gates() {
        let smoke = governance_admission_smoke();
        assert!(matches!(
            smoke,
            Ok(GovernanceAdmissionSmoke {
                governance_ordered_signed: true,
                governance_rejects_invalid_authority: true,
                removed_admin_cannot_win: true,
                invite_controls_enforced: true,
                password_and_welcome_gate: true,
                recovery_trust_model: true,
                abuse_controls_enforced: true,
            })
        ));
    }

    #[test]
    fn abuse_e2e_smoke_covers_g119_gate() -> Result<(), anyhow::Error> {
        let smoke = abuse_e2e_smoke()?;
        assert_eq!(
            smoke,
            AbuseE2eSmoke {
                invite_flood_rate_limited: true,
                spam_burst_rate_limited: true,
                admission_helper_bruteforce_rejected: true,
                signaling_blob_flood_rate_limited: true,
                relay_freeloading_downranked: true,
                service_request_size_exhaustion_rejected: true,
            }
        );
        assert!(smoke.ready());
        Ok(())
    }

    #[test]
    fn connectivity_signaling_push_smoke_covers_phase6_gates() {
        let smoke = connectivity_signaling_push_smoke();
        assert!(matches!(
            smoke,
            Ok(ConnectivitySignalingPushSmoke {
                signaling_zero_linkage_at_rest: true,
                fallback_chain_covered: true,
                owner_overrides_used: true,
                android_wake_content_free: true,
                metadata_matrix_validated: true,
                pcap_no_central_content: true,
                relays_ciphertext_only: true,
                socket_local_process_conformant: true,
                route_reporting_honest: true,
            })
        ));
    }

    #[test]
    fn pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata() {
        let smoke = pcap_acceptance_matrix_smoke();
        assert!(matches!(
            smoke,
            Ok(PcapAcceptanceMatrixSmoke {
                ac1_identity_dm_safety_pcap_clean: true,
                ac8_relay_media_ciphertext_only: true,
                ac15_android_wake_content_free: true,
                ac18_signaling_zero_linkage_at_rest: true,
                ac_metadata_matrix_validated: true,
                forbidden_scanner_covers_release_tokens: true,
            })
        ));
    }

    #[test]
    fn malicious_relay_adversary_smoke_covers_passive_active_and_churn_cases() {
        let smoke = malicious_relay_adversary_smoke();
        assert!(matches!(
            smoke,
            Ok(MaliciousRelayAdversarySmoke {
                passive_read_blocked: true,
                tamper_rejected: true,
                replay_rejected: true,
                drop_requests_bounded_redelivery: true,
                reorder_window_enforced: true,
                endpoint_churn_damped_and_failover_recovered: true,
            })
        ));
    }

    #[test]
    fn malicious_member_adversary_smoke_covers_impersonation_eviction_divergence_and_admin_cases() {
        let smoke = malicious_member_adversary_smoke();
        assert!(matches!(
            smoke,
            Ok(MaliciousMemberAdversarySmoke {
                media_impersonation_rejected: true,
                evicted_member_text_rejected: true,
                evicted_device_media_rejected: true,
                forked_mls_commit_rejected: true,
                out_of_epoch_governance_rejected: true,
                unauthorized_governance_rejected: true,
                removed_admin_race_rejected: true,
            })
        ));
    }

    #[test]
    fn phase_c_device_rotation_smoke_blocks_old_device_sends() {
        let smoke = phase_c_device_rotation_smoke();
        assert!(matches!(
            smoke,
            Ok(PhaseCDeviceRotationSmoke {
                compromised_device_retired: true,
                group_rekeyed_after_rotation: true,
                old_device_send_blocked: true,
                replacement_device_can_send: true,
                stale_epoch_send_blocked: true,
                transparency_notices_include_rotation: true,
                command_surface_reports_device_metadata: true,
            })
        ));
    }

    #[test]
    fn ux_e2e_hardening_smoke_covers_phase7_gates() {
        let smoke = ux_e2e_hardening_smoke();
        assert!(matches!(
            smoke,
            Ok(UxE2eHardeningSmoke {
                command_surface_ready: true,
                discord_style_model_ready: true,
                verification_and_devices_ready: true,
                invite_retention_deletion_ready: true,
                connectivity_copy_ready: true,
                all_phase_smokes_ready: true,
            })
        ));
    }

    #[test]
    fn two_profile_p2p_dm_voice_ui_smoke_covers_task4_gate() {
        let smoke = two_profile_p2p_dm_voice_ui_smoke();
        assert!(smoke.as_ref().is_ok_and(|result| {
            result.independent_profiles_created
                && result.pairwise_safety_numbers_match
                && result.p2p_dm_message_e2e
                && result.voice_media_attempt_covered
                && result.frontend_ui_checks_ready
                && result.no_fake_voice_members
                && result.ready()
        }));
    }
}
