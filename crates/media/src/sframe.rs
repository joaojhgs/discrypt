//! SFrame-like media protection with MLS leaf/device sender binding.
//!
//! This is intentionally small and headless-testable for Phase 1. It is not a
//! full RFC 9605 implementation yet; it preserves the release-critical contract:
//! per-sender/per-device media keys derived from the MLS exporter, authenticated
//! frame metadata, replay rejection, and no raw key export to JS.

use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, Key, KeyInit, Nonce};
use mls_core::{derive_epoch_secret, ExportLabel};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use zeroize::Zeroize;

const DEFAULT_REPLAY_WINDOW: u64 = 64;
const NONCE_LEN: usize = 12;

/// Media sender binding from KID to MLS leaf/device.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SenderBinding {
    /// Media key identifier carried in the protected frame header.
    pub kid: Vec<u8>,
    /// MLS leaf index for the sender device.
    pub leaf_index: u32,
    /// Stable sender device identifier from the device set.
    pub device_id: String,
}

impl SenderBinding {
    /// Validate that the binding is usable as an authenticated media sender id.
    pub fn validate(&self) -> Result<(), MediaError> {
        if self.kid.is_empty() || self.device_id.trim().is_empty() {
            return Err(MediaError::InvalidBinding);
        }
        Ok(())
    }
}

/// Errors from media protection and verification.
#[derive(Debug, Error, Eq, PartialEq)]
pub enum MediaError {
    /// The KID is not registered to an MLS leaf/device binding.
    #[error("unknown sender binding")]
    UnknownSender,
    /// The frame counter was already accepted or fell outside the replay window.
    #[error("replay detected")]
    Replay,
    /// Ciphertext authentication failed.
    #[error("frame authentication failed")]
    AuthenticationFailed,
    /// A sender key was used with a different KID/leaf/device binding.
    #[error("media key does not match sender binding")]
    BindingMismatch,
    /// KID is already registered to a sender binding.
    #[error("duplicate media key id")]
    DuplicateKid,
    /// KID or device binding fields are empty.
    #[error("invalid sender binding")]
    InvalidBinding,
    /// Sender counter exhausted its u64 range.
    #[error("sender counter exhausted")]
    CounterOverflow,
    /// Captured audio did not match the production voice media contract.
    #[error("invalid audio frame: {0}")]
    InvalidAudioFrame(String),
    /// Opus encoding failed before SFrame protection.
    #[error("opus encode failed: {0}")]
    OpusEncodeFailed(String),
    /// Protected media frame could not be handed to the transport sink.
    #[error("media transport failed: {0}")]
    MediaTransportFailed(String),
}

/// Protected media frame passed through relays and JS transform plumbing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProtectedFrame {
    /// Key id identifying the MLS-bound sender key.
    pub kid: Vec<u8>,
    /// Sender monotonic frame counter.
    pub counter: u64,
    /// Authenticated ciphertext bytes.
    pub ciphertext: Vec<u8>,
}

/// Plaintext returned only after KID binding, AEAD authentication, and replay checks.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VerifiedFrame {
    /// Authenticated sender binding.
    pub binding: SenderBinding,
    /// Accepted frame counter.
    pub counter: u64,
    /// Decrypted encoded frame payload.
    pub plaintext: Vec<u8>,
}

/// MLS-exporter-derived media key. The raw key has no public accessor.
pub struct SFrameKey {
    bytes: [u8; 32],
    kid: Vec<u8>,
    binding_hash: [u8; 32],
}

impl core::fmt::Debug for SFrameKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SFrameKey")
            .field("kid", &hex::encode(&self.kid))
            .field("binding_hash", &hex::encode(self.binding_hash))
            .field("raw_key", &"<redacted>")
            .finish()
    }
}

impl Drop for SFrameKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

impl SFrameKey {
    /// Derive a media key for exactly one MLS leaf/device sender binding.
    pub fn derive(epoch_secret: &[u8], binding: &SenderBinding) -> Result<Self, MediaError> {
        binding.validate()?;
        let context = binding_context(binding);
        Ok(Self {
            bytes: derive_epoch_secret(epoch_secret, ExportLabel::Media, &context),
            kid: binding.kid.clone(),
            binding_hash: binding_hash(binding),
        })
    }

    /// Return a non-secret fingerprint useful for diagnostics and tests.
    #[must_use]
    pub fn fingerprint(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(b"discrypt-sframe-key-fingerprint-v1");
        h.update(self.binding_hash);
        h.update(self.bytes);
        h.finalize().into()
    }

    fn protect(
        &self,
        binding: &SenderBinding,
        counter: u64,
        plaintext: &[u8],
    ) -> Result<ProtectedFrame, MediaError> {
        self.verify_binding(binding)?;
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.bytes));
        let nonce_bytes = nonce_for(&self.binding_hash, counter);
        let aad = frame_aad(binding, counter);
        let ciphertext = cipher
            .encrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| MediaError::AuthenticationFailed)?;
        Ok(ProtectedFrame {
            kid: binding.kid.clone(),
            counter,
            ciphertext,
        })
    }

    fn open(&self, binding: &SenderBinding, frame: &ProtectedFrame) -> Result<Vec<u8>, MediaError> {
        self.verify_binding(binding)?;
        if frame.kid != self.kid {
            return Err(MediaError::BindingMismatch);
        }
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&self.bytes));
        let nonce_bytes = nonce_for(&self.binding_hash, frame.counter);
        let aad = frame_aad(binding, frame.counter);
        cipher
            .decrypt(
                Nonce::from_slice(&nonce_bytes),
                Payload {
                    msg: &frame.ciphertext,
                    aad: &aad,
                },
            )
            .map_err(|_| MediaError::AuthenticationFailed)
    }

    fn verify_binding(&self, binding: &SenderBinding) -> Result<(), MediaError> {
        binding.validate()?;
        if self.kid == binding.kid && self.binding_hash == binding_hash(binding) {
            Ok(())
        } else {
            Err(MediaError::BindingMismatch)
        }
    }
}

/// Convenience helper preserving the Phase-0 facade name.
pub fn derive_media_key(
    epoch_secret: &[u8],
    binding: &SenderBinding,
) -> Result<SFrameKey, MediaError> {
    SFrameKey::derive(epoch_secret, binding)
}

/// Convenience helper preserving the Phase-0 facade name while adding AEAD auth.
pub fn protect_frame(
    key: &SFrameKey,
    binding: &SenderBinding,
    counter: u64,
    plaintext: &[u8],
) -> Result<ProtectedFrame, MediaError> {
    key.protect(binding, counter, plaintext)
}

/// Sender-side state that owns the counter and never exposes raw key bytes.
pub struct SFrameSender {
    binding: SenderBinding,
    key: SFrameKey,
    next_counter: u64,
}

impl core::fmt::Debug for SFrameSender {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SFrameSender")
            .field("binding", &self.binding)
            .field("next_counter", &self.next_counter)
            .finish_non_exhaustive()
    }
}

impl SFrameSender {
    /// Create sender state from an MLS exporter secret and sender binding.
    pub fn new(epoch_secret: &[u8], binding: SenderBinding) -> Result<Self, MediaError> {
        let key = SFrameKey::derive(epoch_secret, &binding)?;
        Ok(Self {
            binding,
            key,
            next_counter: 0,
        })
    }

    /// Protect the next encoded media frame.
    pub fn protect(&mut self, plaintext: &[u8]) -> Result<ProtectedFrame, MediaError> {
        let frame = self
            .key
            .protect(&self.binding, self.next_counter, plaintext)?;
        self.next_counter = self
            .next_counter
            .checked_add(1)
            .ok_or(MediaError::CounterOverflow)?;
        Ok(frame)
    }

    /// Current sender binding.
    #[must_use]
    pub fn binding(&self) -> &SenderBinding {
        &self.binding
    }
}

struct BoundMediaKey {
    binding: SenderBinding,
    key: SFrameKey,
}

/// Receiver registry for MLS-signed `KID → leaf/device` media sender state.
#[derive(Default)]
pub struct MediaKeyRegistry {
    keys: BTreeMap<Vec<u8>, BoundMediaKey>,
}

impl core::fmt::Debug for MediaKeyRegistry {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MediaKeyRegistry")
            .field(
                "kids",
                &self.keys.keys().map(hex::encode).collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl MediaKeyRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one KID binding and derive its receiver key from the MLS exporter secret.
    pub fn register_sender(
        &mut self,
        epoch_secret: &[u8],
        binding: SenderBinding,
    ) -> Result<(), MediaError> {
        if self.keys.contains_key(&binding.kid) {
            return Err(MediaError::DuplicateKid);
        }
        let key = SFrameKey::derive(epoch_secret, &binding)?;
        self.keys
            .insert(binding.kid.clone(), BoundMediaKey { binding, key });
        Ok(())
    }

    /// Read the MLS leaf/device binding for a KID without exposing key bytes.
    #[must_use]
    pub fn binding_for_kid(&self, kid: &[u8]) -> Option<&SenderBinding> {
        self.keys.get(kid).map(|bound| &bound.binding)
    }

    fn open(&self, frame: &ProtectedFrame) -> Result<VerifiedFrame, MediaError> {
        let bound = self.keys.get(&frame.kid).ok_or(MediaError::UnknownSender)?;
        let plaintext = bound.key.open(&bound.binding, frame)?;
        Ok(VerifiedFrame {
            binding: bound.binding.clone(),
            counter: frame.counter,
            plaintext,
        })
    }
}

#[derive(Clone, Debug)]
struct ReplayState {
    max_seen: u64,
    seen: BTreeSet<u64>,
}

/// Receiver anti-replay window scoped per sender KID.
#[derive(Clone, Debug)]
pub struct ReplayWindow {
    window_size: u64,
    seen_by_kid: BTreeMap<Vec<u8>, ReplayState>,
}

impl Default for ReplayWindow {
    fn default() -> Self {
        Self::new(DEFAULT_REPLAY_WINDOW)
    }
}

impl ReplayWindow {
    /// Create a replay window. Size 0 is normalized to 1.
    #[must_use]
    pub fn new(window_size: u64) -> Self {
        Self {
            window_size: window_size.max(1),
            seen_by_kid: BTreeMap::new(),
        }
    }

    /// Accept a counter for a KID or reject duplicates/stale frames.
    pub fn accept(&mut self, kid: &[u8], counter: u64) -> Result<(), MediaError> {
        let state = self.seen_by_kid.entry(kid.to_vec()).or_insert(ReplayState {
            max_seen: counter,
            seen: BTreeSet::new(),
        });

        let floor = state
            .max_seen
            .saturating_sub(self.window_size.saturating_sub(1));
        if counter < floor || state.seen.contains(&counter) {
            return Err(MediaError::Replay);
        }

        if counter > state.max_seen {
            state.max_seen = counter;
        }
        let new_floor = state
            .max_seen
            .saturating_sub(self.window_size.saturating_sub(1));
        state.seen.retain(|seen| *seen >= new_floor);
        state.seen.insert(counter);
        Ok(())
    }
}

/// Receiver that authenticates, decrypts, binds sender identity, then consumes replay state.
#[derive(Debug, Default)]
pub struct SFrameReceiver {
    registry: MediaKeyRegistry,
    replay: ReplayWindow,
}

impl SFrameReceiver {
    /// Construct a receiver from a KID registry and replay window.
    #[must_use]
    pub fn new(registry: MediaKeyRegistry, replay: ReplayWindow) -> Self {
        Self { registry, replay }
    }

    /// Open a frame only after AEAD authentication and anti-replay validation.
    pub fn open(&mut self, frame: &ProtectedFrame) -> Result<VerifiedFrame, MediaError> {
        let verified = self.registry.open(frame)?;
        self.replay.accept(&frame.kid, frame.counter)?;
        Ok(verified)
    }

    /// Receiver registry.
    #[must_use]
    pub fn registry(&self) -> &MediaKeyRegistry {
        &self.registry
    }
}

fn binding_context(binding: &SenderBinding) -> Vec<u8> {
    let mut context = Vec::new();
    context.extend_from_slice(b"discrypt-sframe-binding-v1");
    context.extend_from_slice(&(binding.kid.len() as u64).to_be_bytes());
    context.extend_from_slice(&binding.kid);
    context.extend_from_slice(&binding.leaf_index.to_be_bytes());
    let device = binding.device_id.as_bytes();
    context.extend_from_slice(&(device.len() as u64).to_be_bytes());
    context.extend_from_slice(device);
    context
}

fn binding_hash(binding: &SenderBinding) -> [u8; 32] {
    Sha256::digest(binding_context(binding)).into()
}

fn nonce_for(binding_hash: &[u8; 32], counter: u64) -> [u8; NONCE_LEN] {
    let mut h = Sha256::new();
    h.update(b"discrypt-sframe-nonce-v1");
    h.update(binding_hash);
    h.update(counter.to_be_bytes());
    let digest: [u8; 32] = h.finalize().into();
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&digest[..NONCE_LEN]);
    nonce
}

fn frame_aad(binding: &SenderBinding, counter: u64) -> Vec<u8> {
    let mut aad = binding_context(binding);
    aad.extend_from_slice(&counter.to_be_bytes());
    aad
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binding(kid: &[u8], leaf_index: u32, device_id: &str) -> SenderBinding {
        SenderBinding {
            kid: kid.to_vec(),
            leaf_index,
            device_id: device_id.to_owned(),
        }
    }

    #[test]
    fn rejects_replay_and_roundtrips_facade_ciphertext() -> Result<(), MediaError> {
        let b = binding(b"kid-alice-laptop", 1, "alice-laptop");
        let mut sender = SFrameSender::new(&[1; 32], b.clone())?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[1; 32], b)?;
        let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

        let ct = sender.protect(b"opus frame")?;
        assert_ne!(ct.ciphertext, b"opus frame");
        let opened = receiver.open(&ct)?;
        assert_eq!(opened.plaintext, b"opus frame");
        assert_eq!(receiver.open(&ct), Err(MediaError::Replay));
        Ok(())
    }

    #[test]
    fn detects_tamper_without_consuming_replay_counter() -> Result<(), MediaError> {
        let b = binding(b"kid-alice-phone", 2, "alice-phone");
        let mut sender = SFrameSender::new(&[2; 32], b.clone())?;
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[2; 32], b)?;
        let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

        let frame = sender.protect(b"voice")?;
        let mut tampered = frame.clone();
        if let Some(first) = tampered.ciphertext.first_mut() {
            *first ^= 0x80;
        }
        assert_eq!(
            receiver.open(&tampered),
            Err(MediaError::AuthenticationFailed)
        );
        assert_eq!(receiver.open(&frame)?.plaintext, b"voice");
        Ok(())
    }

    #[test]
    fn binds_key_to_leaf_and_device() -> Result<(), MediaError> {
        let laptop = binding(b"shared-kid", 3, "laptop");
        let phone = binding(b"shared-kid", 3, "phone");
        let laptop_key = derive_media_key(&[3; 32], &laptop)?;

        assert_ne!(
            laptop_key.fingerprint(),
            derive_media_key(&[3; 32], &phone)?.fingerprint()
        );
        assert_eq!(
            protect_frame(&laptop_key, &phone, 0, b"bad"),
            Err(MediaError::BindingMismatch)
        );
        Ok(())
    }

    #[test]
    fn passive_relay_has_ciphertext_only_and_unknown_kid_cannot_open() -> Result<(), MediaError> {
        let b = binding(b"kid-bob-desktop", 4, "bob-desktop");
        let mut sender = SFrameSender::new(&[4; 32], b.clone())?;
        let relayed = sender.protect(b"secret media")?;
        assert!(!relayed
            .ciphertext
            .windows(b"secret".len())
            .any(|window| window == b"secret"));

        let empty_registry = MediaKeyRegistry::new();
        let mut relay_without_binding =
            SFrameReceiver::new(empty_registry, ReplayWindow::default());
        assert_eq!(
            relay_without_binding.open(&relayed),
            Err(MediaError::UnknownSender)
        );
        Ok(())
    }

    #[test]
    fn key_debug_and_protected_frame_do_not_expose_raw_secret_or_plaintext(
    ) -> Result<(), MediaError> {
        let b = binding(b"kid-redaction", 8, "alice-desktop");
        let key = derive_media_key(&[8; 32], &b)?;
        let debug = format!("{key:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret media"));

        let frame = protect_frame(&key, &b, 0, b"secret media")?;
        let serialized =
            serde_json::to_vec(&frame).map_err(|_| MediaError::AuthenticationFailed)?;
        assert!(!serialized
            .windows(b"secret media".len())
            .any(|window| window == b"secret media"));
        Ok(())
    }

    #[test]
    fn registry_rejects_duplicate_kid_binding() -> Result<(), MediaError> {
        let first = binding(b"kid-conflict", 1, "laptop");
        let second = binding(b"kid-conflict", 2, "phone");
        let mut registry = MediaKeyRegistry::new();
        registry.register_sender(&[5; 32], first)?;
        assert_eq!(
            registry.register_sender(&[5; 32], second),
            Err(MediaError::DuplicateKid)
        );
        Ok(())
    }

    #[test]
    fn replay_window_accepts_out_of_order_once_and_rejects_stale() {
        let mut replay = ReplayWindow::new(4);
        assert_eq!(replay.accept(b"kid", 10), Ok(()));
        assert_eq!(replay.accept(b"kid", 8), Ok(()));
        assert_eq!(replay.accept(b"kid", 8), Err(MediaError::Replay));
        assert_eq!(replay.accept(b"kid", 15), Ok(()));
        assert_eq!(replay.accept(b"kid", 10), Err(MediaError::Replay));
    }
}
