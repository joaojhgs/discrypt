//! Delivery, ordering, fork detection, Welcome/catch-up, and repair facades around MLS state.
//!
//! This crate deliberately models the service layer around MLS rather than a
//! replacement for MLS cryptography: it orders application events, detects
//! divergent epoch summaries, rejects replay/downgrade/forked commits, and
//! produces explicit rejoin/reproposal repair plans.
//!
//! ## ProductionStatus
//! See [`production_status`] for this crate's build-time gate status. Default
//! builds keep `harness` and `local-dev` disabled; production claims require the
//! explicit `production-network`, `production-media`, or `production-storage`
//! feature matching the claimed runtime capability.

pub mod production_status;
use aes_gcm::aead::{Aead, Payload};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Delivery errors.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum DeliveryError {
    /// A same-epoch summary has a different tree hash or confirmation tag.
    #[error("divergent tree hash at epoch {0}")]
    DivergentTree(u64),
    /// A commit attempts to move the client backwards.
    #[error("downgrade or replay at epoch {0}")]
    DowngradeOrReplay(u64),
    /// A Welcome package is expired.
    #[error("welcome expired")]
    WelcomeExpired,
    /// A repair plan attempted to replay invalid divergent MLS commits.
    #[error("repair attempted to replay divergent MLS commits")]
    DivergentCommitReplay,
    /// A repair plan would move to an older epoch.
    #[error("repair target epoch {target} is older than local epoch {local}")]
    StaleRepairTarget { local: u64, target: u64 },
    /// Text message envelope is malformed or missing required production metadata.
    #[error("invalid text message envelope: {0}")]
    InvalidTextMessageEnvelope(String),
    /// Text message envelope signature verification failed.
    #[error("text message envelope signature verification failed")]
    TextMessageSignatureVerificationFailed,
    /// Text delivery receipt is malformed or missing authenticated metadata.
    #[error("invalid text delivery receipt: {0}")]
    InvalidTextDeliveryReceipt(String),
    /// Text delivery receipt signature verification failed.
    #[error("text delivery receipt signature verification failed")]
    TextDeliveryReceiptSignatureVerificationFailed,
    /// Text payload encryption failed.
    #[error("text message encryption failed")]
    TextMessageEncryptionFailed,
    /// Outbound text route is not ciphertext-only.
    #[error("text outbound route is not ciphertext-only")]
    TextOutboundRouteNotCiphertextOnly,
    /// Outbound text pipeline adapter failed.
    #[error("text outbound adapter failed: {0}")]
    TextOutboundAdapter(String),
    /// Text receive rejected an unauthorized sender.
    #[error("text receive sender leaf {0} is not a member of the current epoch")]
    TextReceiveUnauthorizedSender(u32),
    /// Text receive rejected a replayed message.
    #[error("text receive replay for sender {sender_leaf} sequence {sequence}")]
    TextReceiveReplay { sender_leaf: u32, sequence: u64 },
    /// Text receive rejected a stale epoch or sequence.
    #[error("text receive downgrade from current epoch {current_epoch} to {envelope_epoch}")]
    TextReceiveDowngrade {
        current_epoch: u64,
        envelope_epoch: u64,
    },
    /// Text receive detected a future/forked epoch.
    #[error("text receive fork from current epoch {current_epoch} to {envelope_epoch}")]
    TextReceiveFork {
        current_epoch: u64,
        envelope_epoch: u64,
    },
    /// Text payload decryption failed.
    #[error("text message decryption failed")]
    TextMessageDecryptionFailed,
    /// Text history merge detected a divergent per-author slot or message id.
    #[error("text history divergence requires repair: {0}")]
    TextHistoryDivergence(String),
}

/// Compact state summary exchanged during catch-up.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct EpochSummary {
    /// MLS epoch number.
    pub epoch: u64,
    /// MLS tree hash for the epoch.
    pub tree_hash: [u8; 32],
    /// MLS confirmation tag for the epoch.
    pub confirmation_tag: [u8; 32],
}

/// Compare two summaries and return whether repair is required.
#[must_use]
pub fn needs_repair(local: &EpochSummary, remote: &EpochSummary) -> bool {
    local.epoch == remote.epoch
        && (local.tree_hash != remote.tree_hash
            || local.confirmation_tag != remote.confirmation_tag)
}

/// Status of a remote summary relative to a local state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ForkStatus {
    /// Same epoch and same hashes.
    InSync,
    /// Remote is ahead; request ordered catch-up.
    NeedsCatchUp { remote_epoch: u64 },
    /// Remote is behind and must not be accepted as current history.
    DowngradeOrReplay { remote_epoch: u64 },
    /// Same epoch but different cryptographic state.
    Diverged(ForkEvidence),
}

/// Evidence captured when a fork is detected.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ForkEvidence {
    /// Local accepted summary.
    pub local: EpochSummary,
    /// Remote conflicting summary.
    pub remote: EpochSummary,
}

/// Detect whether a remote summary is a catch-up source, replay/downgrade, or fork.
#[must_use]
pub fn detect_fork_or_replay(local: &EpochSummary, remote: &EpochSummary) -> ForkStatus {
    match remote.epoch.cmp(&local.epoch) {
        Ordering::Greater => ForkStatus::NeedsCatchUp {
            remote_epoch: remote.epoch,
        },
        Ordering::Less => ForkStatus::DowngradeOrReplay {
            remote_epoch: remote.epoch,
        },
        Ordering::Equal if needs_repair(local, remote) => ForkStatus::Diverged(ForkEvidence {
            local: local.clone(),
            remote: remote.clone(),
        }),
        Ordering::Equal => ForkStatus::InSync,
    }
}

/// Current text message envelope schema version.
pub const TEXT_MESSAGE_ENVELOPE_VERSION: u8 = 1;

/// Retention metadata authenticated with every encrypted text message.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextRetentionMetadata {
    /// Human/UX policy label that selected the retention behavior.
    pub policy: String,
    /// Deterministic creation timestamp in milliseconds.
    pub created_at_ms: u64,
    /// Optional expiry timestamp in milliseconds.
    pub expires_at_ms: Option<u64>,
    /// Whether the recipient should delete cached plaintext after first read.
    pub delete_after_read: bool,
}

impl TextRetentionMetadata {
    /// Build retention metadata that can be authenticated by the envelope.
    #[must_use]
    pub fn new(
        policy: impl Into<String>,
        created_at_ms: u64,
        expires_at_ms: Option<u64>,
        delete_after_read: bool,
    ) -> Self {
        Self {
            policy: policy.into(),
            created_at_ms,
            expires_at_ms,
            delete_after_read,
        }
    }

    fn validate(&self) -> Result<(), DeliveryError> {
        if self.policy.trim().is_empty() {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "retention policy is required".to_owned(),
            ));
        }
        if self
            .expires_at_ms
            .is_some_and(|expires_at_ms| expires_at_ms < self.created_at_ms)
        {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "retention expiry is before creation".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Required unsigned fields for a text message envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextMessageEnvelopeInput {
    /// MLS epoch that produced the text exporter/content key.
    pub epoch: u64,
    /// Sender MLS leaf in the epoch.
    pub sender_leaf: u32,
    /// Stable sender device id under the account identity.
    pub sender_device_id: String,
    /// Per-author monotonic message sequence.
    pub sequence: u64,
    /// Stable message id used by history, receipts, and dedupe.
    pub message_id: String,
    /// Authenticated retention/shred metadata for this message.
    pub retention: TextRetentionMetadata,
    /// Encrypted text payload bytes. Plaintext must never be embedded here.
    pub content_ciphertext: Vec<u8>,
}

/// Authenticated, ciphertext-only text message envelope carried by transport/history.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextMessageEnvelope {
    /// Schema version for forwards-compatible parsing.
    pub version: u8,
    /// Content-hiding commitment to the group id; the raw group id is not relay visible.
    pub group_id_commitment: [u8; 32],
    /// MLS epoch that produced the text exporter/content key.
    pub epoch: u64,
    /// Sender MLS leaf in the epoch.
    pub sender_leaf: u32,
    /// Stable sender device id under the account identity.
    pub sender_device_id: String,
    /// Per-author monotonic message sequence.
    pub sequence: u64,
    /// Stable message id used by history, receipts, and dedupe.
    pub message_id: String,
    /// Authenticated retention/shred metadata for this message.
    pub retention: TextRetentionMetadata,
    /// Encrypted text payload bytes. Plaintext must never be embedded here.
    pub content_ciphertext: Vec<u8>,
    /// Ed25519 signature over the canonical unsigned envelope bytes.
    pub signature: Vec<u8>,
}

/// Required unsigned fields for a delivery receipt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextDeliveryReceiptInput {
    /// Stable message id being acknowledged.
    pub message_id: String,
    /// Recipient MLS leaf that verified/persisted the envelope.
    pub recipient_leaf: u32,
    /// Stable recipient device id under the account identity.
    pub recipient_device_id: String,
    /// Deterministic receipt timestamp in milliseconds.
    pub received_at_ms: u64,
    /// Ciphertext hash of the received signed envelope.
    pub envelope_ciphertext_hash: [u8; 32],
}

/// Signed delivery receipt that can justify a remote-delivered UI state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TextDeliveryReceipt {
    /// Content-hiding commitment to the group id.
    pub group_id_commitment: [u8; 32],
    /// Stable message id being acknowledged.
    pub message_id: String,
    /// Recipient MLS leaf that verified/persisted the envelope.
    pub recipient_leaf: u32,
    /// Stable recipient device id under the account identity.
    pub recipient_device_id: String,
    /// Deterministic receipt timestamp in milliseconds.
    pub received_at_ms: u64,
    /// Ciphertext hash of the received signed envelope.
    pub envelope_ciphertext_hash: [u8; 32],
    /// Ed25519 signature over canonical receipt bytes.
    pub signature: Vec<u8>,
}

impl TextDeliveryReceipt {
    /// Create a receipt for an envelope that was actually validated and persisted.
    pub fn sign(
        group_id: &str,
        input: TextDeliveryReceiptInput,
        signing_key: &SigningKey,
    ) -> Result<Self, DeliveryError> {
        let mut receipt = Self {
            group_id_commitment: group_id_commitment(group_id)?,
            message_id: input.message_id,
            recipient_leaf: input.recipient_leaf,
            recipient_device_id: input.recipient_device_id,
            received_at_ms: input.received_at_ms,
            envelope_ciphertext_hash: input.envelope_ciphertext_hash,
            signature: Vec::new(),
        };
        receipt.validate_unsigned()?;
        receipt.signature = signing_key
            .sign(&receipt.canonical_unsigned_bytes())
            .to_bytes()
            .to_vec();
        Ok(receipt)
    }

    /// Verify group binding, message binding, ciphertext hash, and recipient signature.
    pub fn verify(
        &self,
        group_id: &str,
        envelope: &TextMessageEnvelope,
        verifying_key: &VerifyingKey,
    ) -> Result<(), DeliveryError> {
        self.validate_unsigned()?;
        if self.group_id_commitment != group_id_commitment(group_id)? {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "group commitment mismatch".to_owned(),
            ));
        }
        if self.message_id != envelope.message_id {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "message id mismatch".to_owned(),
            ));
        }
        if self.envelope_ciphertext_hash != envelope.ciphertext_hash() {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "ciphertext hash mismatch".to_owned(),
            ));
        }
        let signature_bytes: [u8; 64] = self.signature.as_slice().try_into().map_err(|_| {
            DeliveryError::InvalidTextDeliveryReceipt("signature must be 64 bytes".to_owned())
        })?;
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(&self.canonical_unsigned_bytes(), &signature)
            .map_err(|_| DeliveryError::TextDeliveryReceiptSignatureVerificationFailed)
    }

    /// Canonical unsigned bytes covered by the receipt signature.
    #[must_use]
    pub fn canonical_unsigned_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        push_bytes(&mut out, b"discrypt-text-delivery-receipt-v1");
        out.extend_from_slice(&self.group_id_commitment);
        push_str(&mut out, &self.message_id);
        out.extend_from_slice(&self.recipient_leaf.to_be_bytes());
        push_str(&mut out, &self.recipient_device_id);
        out.extend_from_slice(&self.received_at_ms.to_be_bytes());
        out.extend_from_slice(&self.envelope_ciphertext_hash);
        out
    }

    fn validate_unsigned(&self) -> Result<(), DeliveryError> {
        if self.group_id_commitment == [0; 32] {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "group commitment is required".to_owned(),
            ));
        }
        if self.message_id.trim().is_empty() {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "message id is required".to_owned(),
            ));
        }
        if self.recipient_device_id.trim().is_empty() {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "recipient device id is required".to_owned(),
            ));
        }
        if self.envelope_ciphertext_hash == [0; 32] {
            return Err(DeliveryError::InvalidTextDeliveryReceipt(
                "ciphertext hash is required".to_owned(),
            ));
        }
        Ok(())
    }
}

impl TextMessageEnvelope {
    /// Create and sign a text message envelope.
    pub fn sign(
        group_id: &str,
        input: TextMessageEnvelopeInput,
        signing_key: &SigningKey,
    ) -> Result<Self, DeliveryError> {
        let mut envelope = Self {
            version: TEXT_MESSAGE_ENVELOPE_VERSION,
            group_id_commitment: group_id_commitment(group_id)?,
            epoch: input.epoch,
            sender_leaf: input.sender_leaf,
            sender_device_id: input.sender_device_id,
            sequence: input.sequence,
            message_id: input.message_id,
            retention: input.retention,
            content_ciphertext: input.content_ciphertext,
            signature: Vec::new(),
        };
        envelope.validate_unsigned()?;
        envelope.signature = signing_key
            .sign(&envelope.canonical_unsigned_bytes())
            .to_bytes()
            .to_vec();
        Ok(envelope)
    }

    /// Verify the group commitment, required metadata, and sender device signature.
    pub fn verify(
        &self,
        group_id: &str,
        verifying_key: &VerifyingKey,
    ) -> Result<(), DeliveryError> {
        self.validate_unsigned()?;
        if self.group_id_commitment != group_id_commitment(group_id)? {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "group commitment mismatch".to_owned(),
            ));
        }
        let signature_bytes: [u8; 64] = self.signature.as_slice().try_into().map_err(|_| {
            DeliveryError::InvalidTextMessageEnvelope("signature must be 64 bytes".to_owned())
        })?;
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key
            .verify(&self.canonical_unsigned_bytes(), &signature)
            .map_err(|_| DeliveryError::TextMessageSignatureVerificationFailed)
    }

    /// Hash used by author-log gossip without exposing ciphertext bytes.
    #[must_use]
    pub fn ciphertext_hash(&self) -> [u8; 32] {
        Sha256::digest(&self.content_ciphertext).into()
    }

    /// True when the relay/history-visible envelope bytes contain a forbidden plaintext sample.
    #[must_use]
    pub fn contains_plaintext_sample(&self, plaintext: &[u8]) -> bool {
        if plaintext.is_empty() || plaintext.len() > self.content_ciphertext.len() {
            return false;
        }
        self.content_ciphertext
            .windows(plaintext.len())
            .any(|window| window == plaintext)
    }

    /// Canonical unsigned bytes covered by the signature.
    #[must_use]
    pub fn canonical_unsigned_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        push_bytes(&mut out, b"discrypt-text-message-envelope-v1");
        out.push(self.version);
        out.extend_from_slice(&self.group_id_commitment);
        out.extend_from_slice(&self.epoch.to_be_bytes());
        out.extend_from_slice(&self.sender_leaf.to_be_bytes());
        push_str(&mut out, &self.sender_device_id);
        out.extend_from_slice(&self.sequence.to_be_bytes());
        push_str(&mut out, &self.message_id);
        push_str(&mut out, &self.retention.policy);
        out.extend_from_slice(&self.retention.created_at_ms.to_be_bytes());
        match self.retention.expires_at_ms {
            Some(expires_at_ms) => {
                out.push(1);
                out.extend_from_slice(&expires_at_ms.to_be_bytes());
            }
            None => out.push(0),
        }
        out.push(u8::from(self.retention.delete_after_read));
        push_bytes(&mut out, &self.content_ciphertext);
        out
    }

    /// Canonical signed bytes carried over text/control transport and durable history.
    #[must_use]
    pub fn canonical_signed_bytes(&self) -> Vec<u8> {
        let mut out = self.canonical_unsigned_bytes();
        push_bytes(&mut out, &self.signature);
        out
    }

    fn validate_unsigned(&self) -> Result<(), DeliveryError> {
        if self.version != TEXT_MESSAGE_ENVELOPE_VERSION {
            return Err(DeliveryError::InvalidTextMessageEnvelope(format!(
                "unsupported version {}",
                self.version
            )));
        }
        if self.group_id_commitment == [0; 32] {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "group commitment is required".to_owned(),
            ));
        }
        if self.sender_device_id.trim().is_empty() {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "sender device id is required".to_owned(),
            ));
        }
        if self.message_id.trim().is_empty() {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "message id is required".to_owned(),
            ));
        }
        if self.content_ciphertext.is_empty() {
            return Err(DeliveryError::InvalidTextMessageEnvelope(
                "content ciphertext is required".to_owned(),
            ));
        }
        self.retention.validate()
    }
}

/// Compute the non-reversible group id commitment stored in text envelopes.
pub fn group_id_commitment(group_id: &str) -> Result<[u8; 32], DeliveryError> {
    if group_id.trim().is_empty() {
        return Err(DeliveryError::InvalidTextMessageEnvelope(
            "group id is required".to_owned(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-text-group-id-v1");
    hasher.update((group_id.len() as u64).to_be_bytes());
    hasher.update(group_id.as_bytes());
    Ok(hasher.finalize().into())
}

fn push_str(out: &mut Vec<u8>, value: &str) {
    push_bytes(out, value.as_bytes());
}

fn push_bytes(out: &mut Vec<u8>, value: &[u8]) {
    out.extend_from_slice(&(value.len() as u64).to_be_bytes());
    out.extend_from_slice(value);
}

/// Request accepted by the outbound text send pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextOutboundRequest {
    /// Raw group id kept inside local Rust services; envelope stores only its commitment.
    pub group_id: String,
    /// Channel id whose history receives this message.
    pub channel_id: String,
    /// MLS epoch used for text exporter/content encryption.
    pub epoch: u64,
    /// Sender MLS leaf in the epoch.
    pub sender_leaf: u32,
    /// Stable sender device id.
    pub sender_device_id: String,
    /// Per-author monotonic sequence.
    pub sequence: u64,
    /// Stable message id.
    pub message_id: String,
    /// Authenticated retention metadata.
    pub retention: TextRetentionMetadata,
    /// Plaintext body that must not cross storage, relay, or UI transport boundaries.
    pub plaintext: Vec<u8>,
    /// Deterministic local send timestamp.
    pub sent_at_ms: u64,
}

/// Ciphertext-only route selected for outbound text/control data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSelectedRoute {
    /// Transport session carrying the frame.
    pub session_id: String,
    /// Human/debug route label, such as direct, overlay, or TURN.
    pub route_label: String,
    /// Number of overlay hops, if the selected route uses relays.
    pub overlay_hops: u8,
    /// True only when every transport/overlay leg is ciphertext-only.
    pub ciphertext_only: bool,
}

/// Durable author-log entry persisted after local send encryption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextAuthorLogEnvelope {
    /// Channel id whose history owns the entry.
    pub channel_id: String,
    /// Full signed encrypted envelope.
    pub envelope: TextMessageEnvelope,
    /// Deterministic local send timestamp.
    pub sent_at_ms: u64,
}

/// Opaque frame handed to text/control transport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextOutboundFrame {
    /// Transport session carrying the frame.
    pub session_id: String,
    /// Route label selected by transport/overlay planning.
    pub route_label: String,
    /// Canonical signed encrypted envelope bytes.
    pub payload: Vec<u8>,
}

/// Send lifecycle event kinds emitted to local UI/command consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextSendEventKind {
    /// Message was encrypted, persisted, and queued for transport.
    Pending,
    /// Message frame was accepted by local transport; remote delivery is not claimed.
    TransportAccepted,
    /// A signed recipient receipt was verified.
    ReceiptVerified,
    /// Send failed; see event error text.
    Error,
}

/// Local event emitted by the outbound text send pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSendEvent {
    /// Message id being reported.
    pub message_id: String,
    /// Event kind.
    pub kind: TextSendEventKind,
    /// Optional adapter/error text for failed sends.
    pub error: Option<String>,
}

impl TextSendEvent {
    fn pending(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            kind: TextSendEventKind::Pending,
            error: None,
        }
    }

    fn transport_accepted(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            kind: TextSendEventKind::TransportAccepted,
            error: None,
        }
    }

    fn error(message_id: &str, error: &DeliveryError) -> Self {
        Self {
            message_id: message_id.to_owned(),
            kind: TextSendEventKind::Error,
            error: Some(error.to_string()),
        }
    }
}

/// Result of a successful outbound text send.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextSendReceipt {
    /// Stable message id.
    pub message_id: String,
    /// Signed encrypted envelope persisted and sent.
    pub envelope: TextMessageEnvelope,
    /// Route selected for the transport frame.
    pub route: TextSelectedRoute,
}

/// Author-log persistence seam used by the outbound text pipeline.
pub trait TextAuthorLogStore {
    /// Persist one signed encrypted author-log envelope.
    fn append_author_log(&mut self, entry: TextAuthorLogEnvelope) -> Result<(), DeliveryError>;
}

/// Text/control transport seam used by the outbound text pipeline.
pub trait TextOutboundTransport {
    /// Send one already-protected text frame over the selected ciphertext-only route.
    fn send_text_frame(
        &mut self,
        route: &TextSelectedRoute,
        frame: TextOutboundFrame,
    ) -> Result<(), DeliveryError>;
}

/// Event sink for local pending/delivered/error send status.
pub trait TextSendEventSink {
    /// Emit one local send lifecycle event.
    fn emit_text_send_event(&mut self, event: TextSendEvent) -> Result<(), DeliveryError>;
}

/// Production text-send coordinator over storage, transport, and local event seams.
pub struct TextOutboundPipeline<'a, L, T, E> {
    author_log: &'a mut L,
    transport: &'a mut T,
    events: &'a mut E,
}

impl<'a, L, T, E> TextOutboundPipeline<'a, L, T, E>
where
    L: TextAuthorLogStore,
    T: TextOutboundTransport,
    E: TextSendEventSink,
{
    /// Bind the pipeline to concrete adapter seams.
    #[must_use]
    pub fn new(author_log: &'a mut L, transport: &'a mut T, events: &'a mut E) -> Self {
        Self {
            author_log,
            transport,
            events,
        }
    }

    /// Encrypt, sign, persist, send, and emit lifecycle events for one text message.
    pub fn send(
        &mut self,
        request: TextOutboundRequest,
        route: TextSelectedRoute,
        text_exporter_secret: &[u8],
        signing_key: &SigningKey,
    ) -> Result<TextSendReceipt, DeliveryError> {
        let message_id = request.message_id.clone();
        match self.send_inner(request, route, text_exporter_secret, signing_key) {
            Ok(receipt) => Ok(receipt),
            Err(error) => {
                let _ = self
                    .events
                    .emit_text_send_event(TextSendEvent::error(&message_id, &error));
                Err(error)
            }
        }
    }

    fn send_inner(
        &mut self,
        request: TextOutboundRequest,
        route: TextSelectedRoute,
        text_exporter_secret: &[u8],
        signing_key: &SigningKey,
    ) -> Result<TextSendReceipt, DeliveryError> {
        if !route.ciphertext_only {
            return Err(DeliveryError::TextOutboundRouteNotCiphertextOnly);
        }
        let aad = text_message_encryption_aad(&request)?;
        let content_key = derive_text_message_content_key(text_exporter_secret, &request);
        let nonce = text_message_nonce(&content_key, &request.message_id, request.sequence);
        let content_ciphertext =
            encrypt_text_plaintext(&content_key, &nonce, &aad, &request.plaintext)?;
        let envelope = TextMessageEnvelope::sign(
            &request.group_id,
            TextMessageEnvelopeInput {
                epoch: request.epoch,
                sender_leaf: request.sender_leaf,
                sender_device_id: request.sender_device_id,
                sequence: request.sequence,
                message_id: request.message_id.clone(),
                retention: request.retention,
                content_ciphertext,
            },
            signing_key,
        )?;
        self.author_log.append_author_log(TextAuthorLogEnvelope {
            channel_id: request.channel_id,
            envelope: envelope.clone(),
            sent_at_ms: request.sent_at_ms,
        })?;
        self.events
            .emit_text_send_event(TextSendEvent::pending(&request.message_id))?;
        self.transport.send_text_frame(
            &route,
            TextOutboundFrame {
                session_id: route.session_id.clone(),
                route_label: route.route_label.clone(),
                payload: envelope.canonical_signed_bytes(),
            },
        )?;
        self.events
            .emit_text_send_event(TextSendEvent::transport_accepted(&request.message_id))?;
        Ok(TextSendReceipt {
            message_id: request.message_id,
            envelope,
            route,
        })
    }
}

/// In-memory author-log adapter for deterministic harnesses and service tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTextAuthorLog {
    /// Persisted entries in append order.
    pub entries: Vec<TextAuthorLogEnvelope>,
}

impl TextAuthorLogStore for InMemoryTextAuthorLog {
    fn append_author_log(&mut self, entry: TextAuthorLogEnvelope) -> Result<(), DeliveryError> {
        self.entries.push(entry);
        Ok(())
    }
}

/// In-memory text transport adapter for deterministic harnesses and service tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTextTransport {
    /// Frames accepted by transport.
    pub frames: Vec<TextOutboundFrame>,
    /// Force next send to fail for error-path verification.
    pub fail_next: bool,
}

impl TextOutboundTransport for InMemoryTextTransport {
    fn send_text_frame(
        &mut self,
        route: &TextSelectedRoute,
        frame: TextOutboundFrame,
    ) -> Result<(), DeliveryError> {
        if !route.ciphertext_only {
            return Err(DeliveryError::TextOutboundRouteNotCiphertextOnly);
        }
        if self.fail_next {
            self.fail_next = false;
            return Err(DeliveryError::TextOutboundAdapter(
                "transport send failed".to_owned(),
            ));
        }
        self.frames.push(frame);
        Ok(())
    }
}

/// In-memory event sink for deterministic harnesses and service tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTextSendEvents {
    /// Emitted lifecycle events in order.
    pub events: Vec<TextSendEvent>,
}

impl TextSendEventSink for InMemoryTextSendEvents {
    fn emit_text_send_event(&mut self, event: TextSendEvent) -> Result<(), DeliveryError> {
        self.events.push(event);
        Ok(())
    }
}

/// Derive a text message content-encryption key from Rust-only exporter material.
#[must_use]
pub fn derive_text_message_content_key(
    text_exporter_secret: &[u8],
    request: &TextOutboundRequest,
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-text-message-content-key-v1");
    push_str_for_hash(&mut h, &request.group_id);
    h.update(request.epoch.to_be_bytes());
    h.update(request.sender_leaf.to_be_bytes());
    push_str_for_hash(&mut h, &request.sender_device_id);
    h.update(request.sequence.to_be_bytes());
    push_str_for_hash(&mut h, &request.message_id);
    h.update((text_exporter_secret.len() as u64).to_be_bytes());
    h.update(text_exporter_secret);
    h.finalize().into()
}

fn text_message_encryption_aad(request: &TextOutboundRequest) -> Result<Vec<u8>, DeliveryError> {
    let mut out = Vec::new();
    push_bytes(&mut out, b"discrypt-text-message-aad-v1");
    out.extend_from_slice(&group_id_commitment(&request.group_id)?);
    out.extend_from_slice(&request.epoch.to_be_bytes());
    out.extend_from_slice(&request.sender_leaf.to_be_bytes());
    push_str(&mut out, &request.sender_device_id);
    out.extend_from_slice(&request.sequence.to_be_bytes());
    push_str(&mut out, &request.message_id);
    push_str(&mut out, &request.retention.policy);
    out.extend_from_slice(&request.retention.created_at_ms.to_be_bytes());
    match request.retention.expires_at_ms {
        Some(expires_at_ms) => {
            out.push(1);
            out.extend_from_slice(&expires_at_ms.to_be_bytes());
        }
        None => out.push(0),
    }
    out.push(u8::from(request.retention.delete_after_read));
    Ok(out)
}

fn text_message_nonce(content_key: &[u8; 32], message_id: &str, sequence: u64) -> [u8; 12] {
    let mut h = Sha256::new();
    h.update(b"discrypt-text-message-nonce-v1");
    h.update(content_key);
    h.update(sequence.to_be_bytes());
    push_str_for_hash(&mut h, message_id);
    let digest = h.finalize();
    let mut nonce = [0; 12];
    nonce.copy_from_slice(&digest[..12]);
    nonce
}

fn encrypt_text_plaintext(
    content_key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    plaintext: &[u8],
) -> Result<Vec<u8>, DeliveryError> {
    let cipher = Aes256Gcm::new_from_slice(content_key)
        .map_err(|_| DeliveryError::TextMessageEncryptionFailed)?;
    cipher
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| DeliveryError::TextMessageEncryptionFailed)
}

fn push_str_for_hash(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

/// Durable recipient-side ciphertext entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextReceivedEnvelope {
    /// Channel id whose history owns the entry.
    pub channel_id: String,
    /// Full signed encrypted envelope.
    pub envelope: TextMessageEnvelope,
    /// Deterministic receive timestamp.
    pub received_at_ms: u64,
}

/// Render state returned after receive processing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextRenderState {
    /// Retention allowed local decryption and plaintext rendering.
    Decrypted(Vec<u8>),
    /// Ciphertext was persisted, but plaintext is locked by retention/live-key policy.
    Locked { reason: String },
}

/// UI-facing receive result after validation and persistence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextRenderableMessage {
    /// Stable message id.
    pub message_id: String,
    /// Sender MLS leaf.
    pub sender_leaf: u32,
    /// Per-author sequence.
    pub sequence: u64,
    /// Render state.
    pub state: TextRenderState,
}

/// Receive lifecycle event kinds emitted to local UI/command consumers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextReceiveEventKind {
    /// A received message was persisted and is renderable or locked.
    Updated,
    /// Receive failed; see error text.
    Error,
}

/// Local receive event emitted by the inbound text pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextReceiveEvent {
    /// Message id being reported.
    pub message_id: String,
    /// Event kind.
    pub kind: TextReceiveEventKind,
    /// Optional adapter/error text for failed receives.
    pub error: Option<String>,
}

impl TextReceiveEvent {
    fn updated(message_id: &str) -> Self {
        Self {
            message_id: message_id.to_owned(),
            kind: TextReceiveEventKind::Updated,
            error: None,
        }
    }

    fn error(message_id: &str, error: &DeliveryError) -> Self {
        Self {
            message_id: message_id.to_owned(),
            kind: TextReceiveEventKind::Error,
            error: Some(error.to_string()),
        }
    }
}

/// Request accepted by the inbound text receive pipeline.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextInboundRequest {
    /// Raw group id kept inside local Rust services; envelope stores only its commitment.
    pub group_id: String,
    /// Channel id whose history receives this message.
    pub channel_id: String,
    /// Current locally accepted MLS epoch.
    pub current_epoch: u64,
    /// Sender leaves authorized in the current epoch.
    pub authorized_sender_leaves: BTreeSet<u32>,
    /// Signed encrypted envelope received from transport/history.
    pub envelope: TextMessageEnvelope,
    /// Deterministic receive timestamp.
    pub received_at_ms: u64,
    /// Whether retention/live-key policy currently allows local plaintext decryption.
    pub retention_allows_decrypt: bool,
}

/// Recipient ciphertext persistence seam used by the inbound text pipeline.
pub trait TextRecipientStore {
    /// Persist one received signed encrypted envelope.
    fn persist_received(&mut self, entry: TextReceivedEnvelope) -> Result<(), DeliveryError>;
}

/// Event sink for local receive update/error status.
pub trait TextReceiveEventSink {
    /// Emit one local receive lifecycle event.
    fn emit_text_receive_event(&mut self, event: TextReceiveEvent) -> Result<(), DeliveryError>;
}

/// Per-channel receive ordering and replay state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextReceiveState {
    seen: BTreeSet<(u32, u64, String)>,
    last_sequence_by_leaf: BTreeMap<u32, u64>,
}

impl TextReceiveState {
    /// Validate and record one envelope's anti-replay/order key.
    pub fn accept(&mut self, envelope: &TextMessageEnvelope) -> Result<(), DeliveryError> {
        let key = (
            envelope.sender_leaf,
            envelope.sequence,
            envelope.message_id.clone(),
        );
        if self.seen.contains(&key) {
            return Err(DeliveryError::TextReceiveReplay {
                sender_leaf: envelope.sender_leaf,
                sequence: envelope.sequence,
            });
        }
        if self
            .last_sequence_by_leaf
            .get(&envelope.sender_leaf)
            .is_some_and(|last| envelope.sequence <= *last)
        {
            return Err(DeliveryError::TextReceiveReplay {
                sender_leaf: envelope.sender_leaf,
                sequence: envelope.sequence,
            });
        }
        self.seen.insert(key);
        self.last_sequence_by_leaf
            .insert(envelope.sender_leaf, envelope.sequence);
        Ok(())
    }
}

/// Production text-receive coordinator over validation, storage, decryption, and events.
pub struct TextInboundPipeline<'a, S, E> {
    state: &'a mut TextReceiveState,
    store: &'a mut S,
    events: &'a mut E,
}

impl<'a, S, E> TextInboundPipeline<'a, S, E>
where
    S: TextRecipientStore,
    E: TextReceiveEventSink,
{
    /// Bind the pipeline to concrete adapter seams.
    #[must_use]
    pub fn new(state: &'a mut TextReceiveState, store: &'a mut S, events: &'a mut E) -> Self {
        Self {
            state,
            store,
            events,
        }
    }

    /// Verify, persist, decrypt-or-lock, and emit a UI update for one received text envelope.
    pub fn receive(
        &mut self,
        request: TextInboundRequest,
        text_exporter_secret: &[u8],
        verifying_key: &VerifyingKey,
    ) -> Result<TextRenderableMessage, DeliveryError> {
        let message_id = request.envelope.message_id.clone();
        match self.receive_inner(request, text_exporter_secret, verifying_key) {
            Ok(renderable) => Ok(renderable),
            Err(error) => {
                let _ = self
                    .events
                    .emit_text_receive_event(TextReceiveEvent::error(&message_id, &error));
                Err(error)
            }
        }
    }

    fn receive_inner(
        &mut self,
        request: TextInboundRequest,
        text_exporter_secret: &[u8],
        verifying_key: &VerifyingKey,
    ) -> Result<TextRenderableMessage, DeliveryError> {
        let envelope = request.envelope;
        envelope.verify(&request.group_id, verifying_key)?;
        if envelope.epoch < request.current_epoch {
            return Err(DeliveryError::TextReceiveDowngrade {
                current_epoch: request.current_epoch,
                envelope_epoch: envelope.epoch,
            });
        }
        if envelope.epoch > request.current_epoch {
            return Err(DeliveryError::TextReceiveFork {
                current_epoch: request.current_epoch,
                envelope_epoch: envelope.epoch,
            });
        }
        if !request
            .authorized_sender_leaves
            .contains(&envelope.sender_leaf)
        {
            return Err(DeliveryError::TextReceiveUnauthorizedSender(
                envelope.sender_leaf,
            ));
        }
        self.state.accept(&envelope)?;
        self.store.persist_received(TextReceivedEnvelope {
            channel_id: request.channel_id,
            envelope: envelope.clone(),
            received_at_ms: request.received_at_ms,
        })?;
        let state = if request.retention_allows_decrypt {
            TextRenderState::Decrypted(decrypt_text_envelope(
                &request.group_id,
                text_exporter_secret,
                &envelope,
            )?)
        } else {
            TextRenderState::Locked {
                reason: "retention policy requires live key before plaintext render".to_owned(),
            }
        };
        self.events
            .emit_text_receive_event(TextReceiveEvent::updated(&envelope.message_id))?;
        Ok(TextRenderableMessage {
            message_id: envelope.message_id,
            sender_leaf: envelope.sender_leaf,
            sequence: envelope.sequence,
            state,
        })
    }
}

/// In-memory recipient store for deterministic harnesses and service tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTextRecipientStore {
    /// Persisted received entries in order.
    pub entries: Vec<TextReceivedEnvelope>,
}

impl TextRecipientStore for InMemoryTextRecipientStore {
    fn persist_received(&mut self, entry: TextReceivedEnvelope) -> Result<(), DeliveryError> {
        self.entries.push(entry);
        Ok(())
    }
}

/// In-memory receive event sink for deterministic harnesses and service tests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InMemoryTextReceiveEvents {
    /// Emitted receive lifecycle events in order.
    pub events: Vec<TextReceiveEvent>,
}

impl TextReceiveEventSink for InMemoryTextReceiveEvents {
    fn emit_text_receive_event(&mut self, event: TextReceiveEvent) -> Result<(), DeliveryError> {
        self.events.push(event);
        Ok(())
    }
}

/// Causal slot for one author's text history.
///
/// The same `(sender_leaf, sequence)` slot may only contain one signed
/// ciphertext envelope. A different envelope in the same slot is a fork that
/// must be repaired explicitly instead of being silently overwritten.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct TextAuthorLogSlot {
    /// Sender MLS leaf.
    pub sender_leaf: u32,
    /// Per-author sequence.
    pub sequence: u64,
}

impl From<&TextMessageEnvelope> for TextAuthorLogSlot {
    fn from(envelope: &TextMessageEnvelope) -> Self {
        Self {
            sender_leaf: envelope.sender_leaf,
            sequence: envelope.sequence,
        }
    }
}

/// History merge lifecycle event kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TextHistoryMergeEventKind {
    /// A new author-log or recipient-cache envelope was accepted.
    Inserted,
    /// An exact duplicate signed envelope was ignored.
    DuplicateSuppressed,
    /// A conflicting slot/message requires repair before it may be accepted.
    RepairRequested,
    /// A recipient cache entry was evicted because the cache reached its bound.
    RecipientCacheEvicted,
}

/// Explicit history merge event emitted for UI/service reconciliation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextHistoryMergeEvent {
    /// Event kind.
    pub kind: TextHistoryMergeEventKind,
    /// Message id involved in the merge decision.
    pub message_id: String,
    /// Sender MLS leaf involved in the merge decision.
    pub sender_leaf: u32,
    /// Per-author sequence involved in the merge decision.
    pub sequence: u64,
    /// Operator-facing repair/dedupe detail.
    pub detail: String,
}

impl TextHistoryMergeEvent {
    fn inserted(envelope: &TextMessageEnvelope) -> Self {
        Self {
            kind: TextHistoryMergeEventKind::Inserted,
            message_id: envelope.message_id.clone(),
            sender_leaf: envelope.sender_leaf,
            sequence: envelope.sequence,
            detail: "accepted into causal history".to_owned(),
        }
    }

    fn duplicate(envelope: &TextMessageEnvelope) -> Self {
        Self {
            kind: TextHistoryMergeEventKind::DuplicateSuppressed,
            message_id: envelope.message_id.clone(),
            sender_leaf: envelope.sender_leaf,
            sequence: envelope.sequence,
            detail: "exact signed envelope duplicate suppressed".to_owned(),
        }
    }

    fn repair(envelope: &TextMessageEnvelope, detail: impl Into<String>) -> Self {
        Self {
            kind: TextHistoryMergeEventKind::RepairRequested,
            message_id: envelope.message_id.clone(),
            sender_leaf: envelope.sender_leaf,
            sequence: envelope.sequence,
            detail: detail.into(),
        }
    }

    fn evicted(entry: &TextReceivedEnvelope) -> Self {
        Self {
            kind: TextHistoryMergeEventKind::RecipientCacheEvicted,
            message_id: entry.envelope.message_id.clone(),
            sender_leaf: entry.envelope.sender_leaf,
            sequence: entry.envelope.sequence,
            detail: "evicted oldest recipient ciphertext cache entry".to_owned(),
        }
    }
}

/// Summary returned from an author-log/recipient-cache merge operation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TextHistoryMergeReport {
    /// Newly accepted envelopes.
    pub inserted: usize,
    /// Exact duplicates suppressed without changing state.
    pub duplicates_suppressed: usize,
    /// Divergent slots/message ids that require repair.
    pub repair_events: usize,
    /// Recipient cache evictions caused by bounded capacity.
    pub evicted_from_recipient_cache: usize,
    /// Ordered explicit events for service/UI reconciliation.
    pub events: Vec<TextHistoryMergeEvent>,
}

impl TextHistoryMergeReport {
    fn push(&mut self, event: TextHistoryMergeEvent) {
        match event.kind {
            TextHistoryMergeEventKind::Inserted => self.inserted += 1,
            TextHistoryMergeEventKind::DuplicateSuppressed => self.duplicates_suppressed += 1,
            TextHistoryMergeEventKind::RepairRequested => self.repair_events += 1,
            TextHistoryMergeEventKind::RecipientCacheEvicted => {
                self.evicted_from_recipient_cache += 1;
            }
        }
        self.events.push(event);
    }
}

/// Stateful merge service for text history replicated across devices/recipients.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TextHistoryMergeState {
    author_log: BTreeMap<TextAuthorLogSlot, TextAuthorLogEnvelope>,
    message_index: BTreeMap<String, TextAuthorLogSlot>,
    recipient_cache_capacity: usize,
    recipient_cache: BTreeMap<String, TextReceivedEnvelope>,
}

impl Default for TextHistoryMergeState {
    fn default() -> Self {
        Self::with_recipient_cache_capacity(256)
    }
}

impl TextHistoryMergeState {
    /// Create merge state with a bounded recipient ciphertext cache.
    #[must_use]
    pub fn with_recipient_cache_capacity(recipient_cache_capacity: usize) -> Self {
        Self {
            author_log: BTreeMap::new(),
            message_index: BTreeMap::new(),
            recipient_cache_capacity: recipient_cache_capacity.max(1),
            recipient_cache: BTreeMap::new(),
        }
    }

    /// Merge author-log envelopes from local devices, recipients, or gossip peers.
    ///
    /// Exact duplicate signed envelopes are suppressed. A conflicting envelope in
    /// the same author sequence slot, or a reused message id in another slot,
    /// emits an explicit repair event and is not accepted.
    pub fn merge_author_log<I>(&mut self, entries: I) -> TextHistoryMergeReport
    where
        I: IntoIterator<Item = TextAuthorLogEnvelope>,
    {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| {
            TextAuthorLogSlot::from(&left.envelope)
                .cmp(&TextAuthorLogSlot::from(&right.envelope))
                .then_with(|| left.envelope.epoch.cmp(&right.envelope.epoch))
                .then_with(|| left.envelope.message_id.cmp(&right.envelope.message_id))
        });

        let mut report = TextHistoryMergeReport::default();
        for entry in entries {
            self.merge_one_author_entry(entry, &mut report);
        }
        report
    }

    fn merge_one_author_entry(
        &mut self,
        entry: TextAuthorLogEnvelope,
        report: &mut TextHistoryMergeReport,
    ) {
        let slot = TextAuthorLogSlot::from(&entry.envelope);
        if let Some(existing) = self.author_log.get(&slot) {
            if existing.envelope.canonical_signed_bytes() == entry.envelope.canonical_signed_bytes()
            {
                report.push(TextHistoryMergeEvent::duplicate(&entry.envelope));
            } else {
                report.push(TextHistoryMergeEvent::repair(
                    &entry.envelope,
                    format!(
                        "author slot fork at leaf {} sequence {}; kept {}, rejected {}",
                        slot.sender_leaf,
                        slot.sequence,
                        existing.envelope.message_id,
                        entry.envelope.message_id
                    ),
                ));
            }
            return;
        }

        if let Some(existing_slot) = self.message_index.get(&entry.envelope.message_id) {
            report.push(TextHistoryMergeEvent::repair(
                &entry.envelope,
                format!(
                    "message id reused in slot {}:{}, rejected slot {}:{}",
                    existing_slot.sender_leaf,
                    existing_slot.sequence,
                    slot.sender_leaf,
                    slot.sequence
                ),
            ));
            return;
        }

        self.message_index
            .insert(entry.envelope.message_id.clone(), slot.clone());
        report.push(TextHistoryMergeEvent::inserted(&entry.envelope));
        self.author_log.insert(slot, entry);
    }

    /// Cache received ciphertext envelopes with duplicate suppression and bounds.
    pub fn merge_received_cache<I>(&mut self, entries: I) -> TextHistoryMergeReport
    where
        I: IntoIterator<Item = TextReceivedEnvelope>,
    {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.received_at_ms, entry.envelope.message_id.clone()));

        let mut report = TextHistoryMergeReport::default();
        for entry in entries {
            self.cache_one_received(entry, &mut report);
        }
        report
    }

    fn cache_one_received(
        &mut self,
        entry: TextReceivedEnvelope,
        report: &mut TextHistoryMergeReport,
    ) {
        if let Some(existing) = self.recipient_cache.get(&entry.envelope.message_id) {
            if existing.envelope.canonical_signed_bytes() == entry.envelope.canonical_signed_bytes()
            {
                report.push(TextHistoryMergeEvent::duplicate(&entry.envelope));
            } else {
                report.push(TextHistoryMergeEvent::repair(
                    &entry.envelope,
                    "recipient cache message-id fork; kept existing ciphertext",
                ));
            }
            return;
        }

        report.push(TextHistoryMergeEvent::inserted(&entry.envelope));
        self.recipient_cache
            .insert(entry.envelope.message_id.clone(), entry);
        self.evict_recipient_cache(report);
    }

    fn evict_recipient_cache(&mut self, report: &mut TextHistoryMergeReport) {
        while self.recipient_cache.len() > self.recipient_cache_capacity {
            let Some(evict_id) = self
                .recipient_cache
                .values()
                .min_by_key(|entry| (entry.received_at_ms, entry.envelope.message_id.clone()))
                .map(|entry| entry.envelope.message_id.clone())
            else {
                break;
            };
            if let Some(evicted) = self.recipient_cache.remove(&evict_id) {
                report.push(TextHistoryMergeEvent::evicted(&evicted));
            }
        }
    }

    /// Causally ordered author-log snapshot.
    #[must_use]
    pub fn author_log_snapshot(&self) -> Vec<TextAuthorLogEnvelope> {
        self.author_log.values().cloned().collect()
    }

    /// Ordered recipient cache snapshot by receive time then message id.
    #[must_use]
    pub fn recipient_cache_snapshot(&self) -> Vec<TextReceivedEnvelope> {
        let mut entries = self.recipient_cache.values().cloned().collect::<Vec<_>>();
        entries.sort_by_key(|entry| (entry.received_at_ms, entry.envelope.message_id.clone()));
        entries
    }
}

/// Decrypt a verified text envelope using the same exporter-derived key used on send.
pub fn decrypt_text_envelope(
    group_id: &str,
    text_exporter_secret: &[u8],
    envelope: &TextMessageEnvelope,
) -> Result<Vec<u8>, DeliveryError> {
    let request = TextOutboundRequest {
        group_id: group_id.to_owned(),
        channel_id: String::new(),
        epoch: envelope.epoch,
        sender_leaf: envelope.sender_leaf,
        sender_device_id: envelope.sender_device_id.clone(),
        sequence: envelope.sequence,
        message_id: envelope.message_id.clone(),
        retention: envelope.retention.clone(),
        plaintext: Vec::new(),
        sent_at_ms: 0,
    };
    let aad = text_message_encryption_aad(&request)?;
    let content_key = derive_text_message_content_key(text_exporter_secret, &request);
    let nonce = text_message_nonce(&content_key, &envelope.message_id, envelope.sequence);
    decrypt_text_ciphertext(&content_key, &nonce, &aad, &envelope.content_ciphertext)
}

fn decrypt_text_ciphertext(
    content_key: &[u8; 32],
    nonce: &[u8; 12],
    aad: &[u8],
    ciphertext: &[u8],
) -> Result<Vec<u8>, DeliveryError> {
    let cipher = Aes256Gcm::new_from_slice(content_key)
        .map_err(|_| DeliveryError::TextMessageDecryptionFailed)?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| DeliveryError::TextMessageDecryptionFailed)
}

/// Application event carried alongside ordered MLS delivery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ApplicationEvent {
    /// Epoch under which the event was authored.
    pub epoch: u64,
    /// Author or committer leaf index in the last common accepted tree.
    pub leaf_index: u32,
    /// Stable event id.
    pub event_id: String,
    /// Opaque application payload bytes.
    pub payload: Vec<u8>,
}

impl ApplicationEvent {
    /// Create an event for deterministic tests and facades.
    #[must_use]
    pub fn new(epoch: u64, leaf_index: u32, event_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            epoch,
            leaf_index,
            event_id: event_id.into(),
            payload,
        }
    }

    /// Content hash used by the canonical comparator.
    #[must_use]
    pub fn content_hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(self.event_id.as_bytes());
        h.update(&self.payload);
        h.finalize().into()
    }
}

/// Canonical key: epoch → committer/author leaf index → signed content hash.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct CanonicalEventKey {
    /// Event epoch.
    pub epoch: u64,
    /// Author/committer leaf index.
    pub leaf_index: u32,
    /// Event content hash.
    pub content_hash: [u8; 32],
}

impl From<&ApplicationEvent> for CanonicalEventKey {
    fn from(event: &ApplicationEvent) -> Self {
        Self {
            epoch: event.epoch,
            leaf_index: event.leaf_index,
            content_hash: event.content_hash(),
        }
    }
}

/// Deterministically order application events by the plan's canonical comparator.
#[must_use]
pub fn order_application_events(mut events: Vec<ApplicationEvent>) -> Vec<ApplicationEvent> {
    events.sort_by(|a, b| {
        CanonicalEventKey::from(a)
            .cmp(&CanonicalEventKey::from(b))
            .then_with(|| a.event_id.cmp(&b.event_id))
    });
    events
}

/// Commit envelope accepted by the delivery layer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitEnvelope {
    /// Summary after applying the commit.
    pub summary: EpochSummary,
    /// Committer leaf index.
    pub committer_leaf: u32,
    /// Valid application events that may be applied after MLS state is accepted.
    pub application_events: Vec<ApplicationEvent>,
}

impl CommitEnvelope {
    /// Build a commit envelope and order application events canonically.
    #[must_use]
    pub fn new(
        summary: EpochSummary,
        committer_leaf: u32,
        application_events: Vec<ApplicationEvent>,
    ) -> Self {
        Self {
            summary,
            committer_leaf,
            application_events: order_application_events(application_events),
        }
    }
}

/// Deterministic delivery state for tests and higher-level facades.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DeliveryState {
    summary: EpochSummary,
    accepted_events: BTreeMap<CanonicalEventKey, ApplicationEvent>,
}

impl DeliveryState {
    /// Create a state from the last accepted MLS summary.
    #[must_use]
    pub fn new(summary: EpochSummary) -> Self {
        Self {
            summary,
            accepted_events: BTreeMap::new(),
        }
    }

    /// Current accepted summary.
    #[must_use]
    pub fn summary(&self) -> &EpochSummary {
        &self.summary
    }

    /// Accepted application events in canonical order.
    #[must_use]
    pub fn accepted_events(&self) -> Vec<ApplicationEvent> {
        self.accepted_events.values().cloned().collect()
    }

    /// Apply a commit only if it extends this state without replay/downgrade/fork.
    pub fn apply_commit(&mut self, commit: CommitEnvelope) -> Result<(), DeliveryError> {
        match detect_fork_or_replay(&self.summary, &commit.summary) {
            ForkStatus::NeedsCatchUp { .. }
                if commit.summary.epoch == self.summary.epoch.saturating_add(1) =>
            {
                if let Some(event) = commit
                    .application_events
                    .iter()
                    .find(|event| event.epoch != commit.summary.epoch)
                {
                    return Err(DeliveryError::DowngradeOrReplay(event.epoch));
                }
                self.summary = commit.summary;
                for event in commit.application_events {
                    self.accepted_events
                        .insert(CanonicalEventKey::from(&event), event);
                }
                Ok(())
            }
            ForkStatus::InSync => Err(DeliveryError::DowngradeOrReplay(commit.summary.epoch)),
            ForkStatus::NeedsCatchUp { remote_epoch } => {
                Err(DeliveryError::DowngradeOrReplay(remote_epoch))
            }
            ForkStatus::DowngradeOrReplay { remote_epoch } => {
                Err(DeliveryError::DowngradeOrReplay(remote_epoch))
            }
            ForkStatus::Diverged(evidence) => {
                Err(DeliveryError::DivergentTree(evidence.local.epoch))
            }
        }
    }

    /// Apply an explicit fork repair plan.
    ///
    /// This models the service contract around OpenMLS repair: losing members
    /// rejoin/reboot to the winning cryptographic state and only application
    /// events re-proposed under that repaired epoch are accepted. Divergent MLS
    /// commits from the losing branch are never replayed.
    pub fn apply_repair_plan(&mut self, plan: RepairPlan) -> Result<(), DeliveryError> {
        if plan.replays_divergent_mls_commits {
            return Err(DeliveryError::DivergentCommitReplay);
        }
        if plan.winner.epoch < self.summary.epoch {
            return Err(DeliveryError::StaleRepairTarget {
                local: self.summary.epoch,
                target: plan.winner.epoch,
            });
        }
        if matches!(plan.action, RepairAction::None) {
            return Ok(());
        }

        for event in &plan.reproposed_events {
            if event.epoch != plan.winner.epoch {
                return Err(DeliveryError::DowngradeOrReplay(event.epoch));
            }
        }

        self.summary = plan.winner;
        for event in plan.reproposed_events {
            self.accepted_events
                .insert(CanonicalEventKey::from(&event), event);
        }
        Ok(())
    }
}

/// Expiring Welcome package for final admission into a current MLS state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WelcomePackage {
    /// Room/group id.
    pub room_id: String,
    /// Newly admitted leaf.
    pub new_leaf: u32,
    /// Accepted epoch summary the new member should join.
    pub summary: EpochSummary,
    /// Expiration timestamp in deterministic milliseconds.
    pub expires_at_ms: u64,
}

impl WelcomePackage {
    /// Build a Welcome package.
    #[must_use]
    pub fn new(
        room_id: impl Into<String>,
        new_leaf: u32,
        summary: EpochSummary,
        expires_at_ms: u64,
    ) -> Self {
        Self {
            room_id: room_id.into(),
            new_leaf,
            summary,
            expires_at_ms,
        }
    }

    /// Validate the Welcome at a deterministic timestamp.
    pub fn validate(&self, now_ms: u64) -> Result<(), DeliveryError> {
        if now_ms <= self.expires_at_ms {
            Ok(())
        } else {
            Err(DeliveryError::WelcomeExpired)
        }
    }
}

/// Catch-up bundle delivered after a Welcome or when a remote member is ahead.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CatchUpBundle {
    /// Latest accepted summary.
    pub summary: EpochSummary,
    /// Ordered commits needed by the receiver.
    pub commits: Vec<CommitEnvelope>,
    /// Ordered application events safe to replay under the accepted MLS state.
    pub application_events: Vec<ApplicationEvent>,
}

impl CatchUpBundle {
    /// Build a bundle with ordered application events.
    #[must_use]
    pub fn new(
        summary: EpochSummary,
        commits: Vec<CommitEnvelope>,
        application_events: Vec<ApplicationEvent>,
    ) -> Self {
        Self {
            summary,
            commits,
            application_events: order_application_events(application_events),
        }
    }
}

/// Repair action: rejoin first, then re-propose valid app events only.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RepairAction {
    /// No repair required.
    None,
    /// Rejoin/reboot MLS state, then re-propose valid application events.
    RejoinAndReproposal { coordinator_leaf: u32 },
}

/// Repair plan for a detected fork.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RepairPlan {
    /// Action to execute.
    pub action: RepairAction,
    /// Winning MLS summary to rejoin.
    pub winner: EpochSummary,
    /// Application-level events eligible for re-proposal after rejoin.
    pub reproposed_events: Vec<ApplicationEvent>,
    /// Explicit marker that divergent MLS commits are not replayed.
    pub replays_divergent_mls_commits: bool,
}

/// Select a deterministic repair coordinator from last common accepted leaf indexes.
#[must_use]
pub fn select_repair_action(diverged: bool, leaves: &[u32]) -> RepairAction {
    if !diverged {
        return RepairAction::None;
    }
    RepairAction::RejoinAndReproposal {
        coordinator_leaf: leaves.iter().copied().max().unwrap_or_default(),
    }
}

/// Build an explicit repair plan for a fork.
#[must_use]
pub fn plan_repair(
    evidence: ForkEvidence,
    last_common_leaves: &[u32],
    still_valid_events: Vec<ApplicationEvent>,
) -> RepairPlan {
    let winner = if evidence.remote > evidence.local {
        evidence.remote
    } else {
        evidence.local
    };
    let winner_epoch = winner.epoch;
    RepairPlan {
        action: select_repair_action(true, last_common_leaves),
        winner,
        reproposed_events: order_application_events(
            still_valid_events
                .into_iter()
                .filter(|event| event.epoch == winner_epoch)
                .collect(),
        ),
        replays_divergent_mls_commits: false,
    }
}

/// Apply a repair plan to all honest members and return their repaired summaries.
#[must_use]
pub fn repair_to_winner(participants: usize, plan: &RepairPlan) -> Vec<EpochSummary> {
    (0..participants).map(|_| plan.winner.clone()).collect()
}

/// Build deterministic test summaries.
#[must_use]
pub fn summary(epoch: u64, tree_byte: u8, confirmation_byte: u8) -> EpochSummary {
    EpochSummary {
        epoch,
        tree_hash: [tree_byte; 32],
        confirmation_tag: [confirmation_byte; 32],
    }
}

/// Assert all summaries converge to the same confirmation tag.
#[must_use]
pub fn equal_confirmation_tags(summaries: &[EpochSummary]) -> bool {
    let Some(first) = summaries.first() else {
        return true;
    };
    summaries
        .iter()
        .all(|summary| summary.confirmation_tag == first.confirmation_tag)
}

/// Dedupe event ids after repair/reproposal.
#[must_use]
pub fn event_ids(events: &[ApplicationEvent]) -> BTreeSet<String> {
    events.iter().map(|event| event.event_id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;

    fn signing_key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn retention() -> TextRetentionMetadata {
        TextRetentionMetadata::new("7 day default", 1_000, Some(605_800_000), false)
    }

    #[test]
    fn text_message_envelope_authenticates_required_metadata() -> Result<(), DeliveryError> {
        let signer = signing_key(7);
        let plaintext = b"hello plaintext";
        let ciphertext = b"ciphertext:sealed-body".to_vec();
        let envelope = TextMessageEnvelope::sign(
            "group/private-lab",
            TextMessageEnvelopeInput {
                epoch: 42,
                sender_leaf: 3,
                sender_device_id: "alice-laptop".to_owned(),
                sequence: 9,
                message_id: "msg-9".to_owned(),
                retention: retention(),
                content_ciphertext: ciphertext.clone(),
            },
            &signer,
        )?;

        assert_eq!(envelope.version, TEXT_MESSAGE_ENVELOPE_VERSION);
        assert_eq!(envelope.epoch, 42);
        assert_eq!(envelope.sender_leaf, 3);
        assert_eq!(envelope.sender_device_id, "alice-laptop");
        assert_eq!(envelope.sequence, 9);
        assert_eq!(envelope.message_id, "msg-9");
        assert_ne!(envelope.group_id_commitment, [0; 32]);
        assert_ne!(
            envelope.group_id_commitment,
            group_id_commitment("other-group")?
        );
        assert_eq!(envelope.content_ciphertext, ciphertext);
        assert!(!envelope.contains_plaintext_sample(plaintext));
        assert_eq!(envelope.signature.len(), 64);
        envelope.verify("group/private-lab", &signer.verifying_key())
    }

    #[test]
    fn text_message_envelope_rejects_tampered_or_missing_fields() -> Result<(), DeliveryError> {
        let signer = signing_key(8);
        let envelope = TextMessageEnvelope::sign(
            "group/private-lab",
            TextMessageEnvelopeInput {
                epoch: 7,
                sender_leaf: 1,
                sender_device_id: "alice-phone".to_owned(),
                sequence: 2,
                message_id: "msg-2".to_owned(),
                retention: retention(),
                content_ciphertext: b"ciphertext".to_vec(),
            },
            &signer,
        )?;

        let mut tampered_epoch = envelope.clone();
        tampered_epoch.epoch = 8;
        assert_eq!(
            tampered_epoch.verify("group/private-lab", &signer.verifying_key()),
            Err(DeliveryError::TextMessageSignatureVerificationFailed)
        );

        let mut tampered_retention = envelope.clone();
        tampered_retention.retention.delete_after_read = true;
        assert_eq!(
            tampered_retention.verify("group/private-lab", &signer.verifying_key()),
            Err(DeliveryError::TextMessageSignatureVerificationFailed)
        );

        assert_eq!(
            envelope.verify("other-group", &signer.verifying_key()),
            Err(DeliveryError::InvalidTextMessageEnvelope(
                "group commitment mismatch".to_owned()
            ))
        );

        assert!(matches!(
            TextMessageEnvelope::sign(
                "group/private-lab",
                TextMessageEnvelopeInput {
                    epoch: 7,
                    sender_leaf: 1,
                    sender_device_id: "alice-phone".to_owned(),
                    sequence: 2,
                    message_id: "msg-2".to_owned(),
                    retention: retention(),
                    content_ciphertext: Vec::new(),
                },
                &signer,
            ),
            Err(DeliveryError::InvalidTextMessageEnvelope(_))
        ));
        Ok(())
    }

    fn outbound_request(message_id: &str) -> TextOutboundRequest {
        TextOutboundRequest {
            group_id: "group/private-lab".to_owned(),
            channel_id: "general".to_owned(),
            epoch: 7,
            sender_leaf: 1,
            sender_device_id: "alice-laptop".to_owned(),
            sequence: 2,
            message_id: message_id.to_owned(),
            retention: retention(),
            plaintext: b"hello plaintext".to_vec(),
            sent_at_ms: 1_234,
        }
    }

    fn selected_route(ciphertext_only: bool) -> TextSelectedRoute {
        TextSelectedRoute {
            session_id: "session-text".to_owned(),
            route_label: "overlay-hop".to_owned(),
            overlay_hops: 2,
            ciphertext_only,
        }
    }

    #[test]
    fn outbound_text_pipeline_persists_sends_and_emits_events() -> Result<(), DeliveryError> {
        let signer = signing_key(9);
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport::default();
        let mut events = InMemoryTextSendEvents::default();
        let mut pipeline = TextOutboundPipeline::new(&mut log, &mut transport, &mut events);
        let receipt = pipeline.send(
            outbound_request("msg-pipeline"),
            selected_route(true),
            b"openmls-text-exporter-secret",
            &signer,
        )?;

        assert_eq!(receipt.message_id, "msg-pipeline");
        assert_eq!(receipt.route.route_label, "overlay-hop");
        assert_eq!(log.entries.len(), 1);
        assert_eq!(log.entries[0].channel_id, "general");
        assert_eq!(log.entries[0].sent_at_ms, 1_234);
        assert_eq!(transport.frames.len(), 1);
        assert_eq!(transport.frames[0].session_id, "session-text");
        assert_eq!(
            transport.frames[0].payload,
            receipt.envelope.canonical_signed_bytes()
        );
        assert!(!receipt
            .envelope
            .contains_plaintext_sample(b"hello plaintext"));
        assert_ne!(receipt.envelope.content_ciphertext, b"hello plaintext");
        receipt
            .envelope
            .verify("group/private-lab", &signer.verifying_key())?;
        assert_eq!(
            events
                .events
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>(),
            vec![
                &TextSendEventKind::Pending,
                &TextSendEventKind::TransportAccepted,
            ]
        );
        Ok(())
    }

    #[test]
    fn text_delivery_receipt_authenticates_remote_delivery_claim() -> Result<(), DeliveryError> {
        let (envelope, _sender) = signed_envelope_for_receive("msg-receipted")?;
        let recipient_signer = signing_key(33);
        let receipt = TextDeliveryReceipt::sign(
            "group/private-lab",
            TextDeliveryReceiptInput {
                message_id: envelope.message_id.clone(),
                recipient_leaf: 2,
                recipient_device_id: "bob-phone".to_owned(),
                received_at_ms: 2_222,
                envelope_ciphertext_hash: envelope.ciphertext_hash(),
            },
            &recipient_signer,
        )?;

        receipt.verify(
            "group/private-lab",
            &envelope,
            &recipient_signer.verifying_key(),
        )?;
        let mut tampered = receipt.clone();
        tampered.message_id = "other-message".to_owned();
        assert!(matches!(
            tampered.verify(
                "group/private-lab",
                &envelope,
                &recipient_signer.verifying_key(),
            ),
            Err(DeliveryError::InvalidTextDeliveryReceipt(_))
        ));
        assert!(matches!(
            receipt.verify("wrong-group", &envelope, &recipient_signer.verifying_key(),),
            Err(DeliveryError::InvalidTextDeliveryReceipt(_))
        ));
        Ok(())
    }

    #[test]
    fn outbound_text_pipeline_emits_error_for_failed_transport() {
        let signer = signing_key(10);
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport {
            frames: Vec::new(),
            fail_next: true,
        };
        let mut events = InMemoryTextSendEvents::default();
        let mut pipeline = TextOutboundPipeline::new(&mut log, &mut transport, &mut events);
        let result = pipeline.send(
            outbound_request("msg-error"),
            selected_route(true),
            b"openmls-text-exporter-secret",
            &signer,
        );

        assert!(matches!(result, Err(DeliveryError::TextOutboundAdapter(_))));
        assert_eq!(log.entries.len(), 1);
        assert!(transport.frames.is_empty());
        assert_eq!(
            events
                .events
                .iter()
                .map(|event| &event.kind)
                .collect::<Vec<_>>(),
            vec![&TextSendEventKind::Pending, &TextSendEventKind::Error]
        );
    }

    #[test]
    fn outbound_text_pipeline_rejects_non_ciphertext_route() {
        let signer = signing_key(11);
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport::default();
        let mut events = InMemoryTextSendEvents::default();
        let mut pipeline = TextOutboundPipeline::new(&mut log, &mut transport, &mut events);
        let result = pipeline.send(
            outbound_request("msg-bad-route"),
            selected_route(false),
            b"openmls-text-exporter-secret",
            &signer,
        );

        assert_eq!(
            result,
            Err(DeliveryError::TextOutboundRouteNotCiphertextOnly)
        );
        assert!(log.entries.is_empty());
        assert!(transport.frames.is_empty());
        assert_eq!(events.events.len(), 1);
        assert_eq!(events.events[0].kind, TextSendEventKind::Error);
    }

    fn authorized_leaves() -> BTreeSet<u32> {
        BTreeSet::from([1, 2])
    }

    fn signed_envelope_for_receive(
        message_id: &str,
    ) -> Result<(TextMessageEnvelope, SigningKey), DeliveryError> {
        let signer = signing_key(12);
        let mut log = InMemoryTextAuthorLog::default();
        let mut transport = InMemoryTextTransport::default();
        let mut events = InMemoryTextSendEvents::default();
        let receipt = TextOutboundPipeline::new(&mut log, &mut transport, &mut events).send(
            outbound_request(message_id),
            selected_route(true),
            b"openmls-text-exporter-secret",
            &signer,
        )?;
        Ok((receipt.envelope, signer))
    }

    #[test]
    fn inbound_text_pipeline_validates_decrypts_persists_and_emits_update(
    ) -> Result<(), DeliveryError> {
        let (envelope, signer) = signed_envelope_for_receive("msg-receive")?;
        let mut state = TextReceiveState::default();
        let mut store = InMemoryTextRecipientStore::default();
        let mut events = InMemoryTextReceiveEvents::default();
        let renderable = TextInboundPipeline::new(&mut state, &mut store, &mut events).receive(
            TextInboundRequest {
                group_id: "group/private-lab".to_owned(),
                channel_id: "general".to_owned(),
                current_epoch: 7,
                authorized_sender_leaves: authorized_leaves(),
                envelope,
                received_at_ms: 2_000,
                retention_allows_decrypt: true,
            },
            b"openmls-text-exporter-secret",
            &signer.verifying_key(),
        )?;

        assert_eq!(renderable.message_id, "msg-receive");
        assert_eq!(renderable.sender_leaf, 1);
        assert_eq!(renderable.sequence, 2);
        assert_eq!(
            renderable.state,
            TextRenderState::Decrypted(b"hello plaintext".to_vec())
        );
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].channel_id, "general");
        assert_eq!(store.entries[0].received_at_ms, 2_000);
        assert_eq!(events.events.len(), 1);
        assert_eq!(events.events[0].kind, TextReceiveEventKind::Updated);
        Ok(())
    }

    #[test]
    fn inbound_text_pipeline_renders_locked_when_retention_blocks_decrypt(
    ) -> Result<(), DeliveryError> {
        let (envelope, signer) = signed_envelope_for_receive("msg-locked")?;
        let mut state = TextReceiveState::default();
        let mut store = InMemoryTextRecipientStore::default();
        let mut events = InMemoryTextReceiveEvents::default();
        let renderable = TextInboundPipeline::new(&mut state, &mut store, &mut events).receive(
            TextInboundRequest {
                group_id: "group/private-lab".to_owned(),
                channel_id: "general".to_owned(),
                current_epoch: 7,
                authorized_sender_leaves: authorized_leaves(),
                envelope,
                received_at_ms: 2_000,
                retention_allows_decrypt: false,
            },
            b"openmls-text-exporter-secret",
            &signer.verifying_key(),
        )?;

        assert!(matches!(renderable.state, TextRenderState::Locked { .. }));
        assert_eq!(store.entries.len(), 1);
        assert_eq!(events.events[0].kind, TextReceiveEventKind::Updated);
        Ok(())
    }

    #[test]
    fn inbound_text_pipeline_rejects_replay_unauthorized_and_epoch_mismatch(
    ) -> Result<(), DeliveryError> {
        let (envelope, signer) = signed_envelope_for_receive("msg-replay")?;
        let mut state = TextReceiveState::default();
        let mut store = InMemoryTextRecipientStore::default();
        let mut events = InMemoryTextReceiveEvents::default();
        let mut pipeline = TextInboundPipeline::new(&mut state, &mut store, &mut events);
        let base_request = TextInboundRequest {
            group_id: "group/private-lab".to_owned(),
            channel_id: "general".to_owned(),
            current_epoch: 7,
            authorized_sender_leaves: authorized_leaves(),
            envelope: envelope.clone(),
            received_at_ms: 2_000,
            retention_allows_decrypt: true,
        };
        assert!(pipeline
            .receive(
                base_request.clone(),
                b"openmls-text-exporter-secret",
                &signer.verifying_key(),
            )
            .is_ok());
        assert_eq!(
            pipeline.receive(
                base_request.clone(),
                b"openmls-text-exporter-secret",
                &signer.verifying_key(),
            ),
            Err(DeliveryError::TextReceiveReplay {
                sender_leaf: 1,
                sequence: 2,
            })
        );

        let mut unauthorized = base_request.clone();
        unauthorized.envelope.message_id = "msg-unauthorized".to_owned();
        unauthorized.envelope.sequence = 3;
        unauthorized.envelope.signature = signing_key(12)
            .sign(&unauthorized.envelope.canonical_unsigned_bytes())
            .to_bytes()
            .to_vec();
        unauthorized.authorized_sender_leaves = BTreeSet::new();
        assert_eq!(
            pipeline.receive(
                unauthorized,
                b"openmls-text-exporter-secret",
                &signer.verifying_key(),
            ),
            Err(DeliveryError::TextReceiveUnauthorizedSender(1))
        );

        let mut stale = base_request;
        stale.current_epoch = 8;
        assert_eq!(
            pipeline.receive(
                stale,
                b"openmls-text-exporter-secret",
                &signer.verifying_key(),
            ),
            Err(DeliveryError::TextReceiveDowngrade {
                current_epoch: 8,
                envelope_epoch: 7,
            })
        );
        assert!(events
            .events
            .iter()
            .any(|event| event.kind == TextReceiveEventKind::Error));
        Ok(())
    }

    fn signed_log_entry(
        sequence: u64,
        message_id: &str,
        ciphertext: &[u8],
    ) -> Result<TextAuthorLogEnvelope, DeliveryError> {
        Ok(TextAuthorLogEnvelope {
            channel_id: "general".to_owned(),
            envelope: TextMessageEnvelope::sign(
                "group/private-lab",
                TextMessageEnvelopeInput {
                    epoch: 7,
                    sender_leaf: 1,
                    sender_device_id: format!("alice-device-{sequence}"),
                    sequence,
                    message_id: message_id.to_owned(),
                    retention: retention(),
                    content_ciphertext: ciphertext.to_vec(),
                },
                &signing_key(42),
            )?,
            sent_at_ms: sequence * 10,
        })
    }

    #[test]
    fn text_history_merge_orders_and_suppresses_duplicates() -> Result<(), DeliveryError> {
        let entry_1 = signed_log_entry(1, "msg-1", b"ciphertext-1")?;
        let entry_2 = signed_log_entry(2, "msg-2", b"ciphertext-2")?;
        let entry_3 = signed_log_entry(3, "msg-3", b"ciphertext-3")?;
        let mut merge = TextHistoryMergeState::default();
        let report = merge.merge_author_log(vec![
            entry_3.clone(),
            entry_1.clone(),
            entry_2.clone(),
            entry_2.clone(),
        ]);

        assert_eq!(report.inserted, 3);
        assert_eq!(report.duplicates_suppressed, 1);
        assert_eq!(report.repair_events, 0);
        assert_eq!(
            merge
                .author_log_snapshot()
                .iter()
                .map(|entry| entry.envelope.message_id.as_str())
                .collect::<Vec<_>>(),
            vec!["msg-1", "msg-2", "msg-3"]
        );
        assert!(report
            .events
            .iter()
            .any(|event| event.kind == TextHistoryMergeEventKind::DuplicateSuppressed));
        Ok(())
    }

    #[test]
    fn text_history_merge_requests_repair_for_divergent_slots_and_ids() -> Result<(), DeliveryError>
    {
        let accepted = signed_log_entry(5, "msg-5", b"ciphertext-a")?;
        let same_slot_fork = signed_log_entry(5, "msg-5-fork", b"ciphertext-b")?;
        let reused_message_id = signed_log_entry(6, "msg-5", b"ciphertext-c")?;
        let mut merge = TextHistoryMergeState::default();
        assert_eq!(merge.merge_author_log(vec![accepted.clone()]).inserted, 1);
        let report =
            merge.merge_author_log(vec![same_slot_fork.clone(), reused_message_id.clone()]);

        assert_eq!(report.inserted, 0);
        assert_eq!(report.repair_events, 2);
        assert_eq!(merge.author_log_snapshot(), vec![accepted]);
        assert!(report.events.iter().all(|event| {
            event.kind == TextHistoryMergeEventKind::RepairRequested
                && event.detail.contains("rejected")
        }));
        Ok(())
    }

    #[test]
    fn text_history_merge_bounds_recipient_cache_and_reports_evictions() -> Result<(), DeliveryError>
    {
        let mut merge = TextHistoryMergeState::with_recipient_cache_capacity(3);
        let received = (0..4)
            .map(|idx| {
                signed_log_entry(idx + 1, &format!("cached-{idx}"), &[idx as u8 + 1]).map(|entry| {
                    TextReceivedEnvelope {
                        channel_id: entry.channel_id,
                        envelope: entry.envelope,
                        received_at_ms: idx,
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let duplicate = received[3].clone();
        let report = merge.merge_received_cache(received.into_iter().chain([duplicate]));

        assert_eq!(report.inserted, 4);
        assert_eq!(report.duplicates_suppressed, 1);
        assert_eq!(report.evicted_from_recipient_cache, 1);
        assert_eq!(
            merge
                .recipient_cache_snapshot()
                .iter()
                .map(|entry| entry.envelope.message_id.as_str())
                .collect::<Vec<_>>(),
            vec!["cached-1", "cached-2", "cached-3"]
        );
        assert!(report
            .events
            .iter()
            .any(|event| event.kind == TextHistoryMergeEventKind::RecipientCacheEvicted));
        Ok(())
    }

    #[test]
    fn orders_events_by_epoch_leaf_and_content_hash() {
        let ordered = order_application_events(vec![
            ApplicationEvent::new(2, 9, "late", b"z".to_vec()),
            ApplicationEvent::new(1, 8, "b", b"b".to_vec()),
            ApplicationEvent::new(1, 2, "a", b"a".to_vec()),
        ]);
        assert_eq!(ordered[0].event_id, "a");
        assert_eq!(ordered[1].event_id, "b");
        assert_eq!(ordered[2].event_id, "late");
    }

    #[test]
    fn detects_same_epoch_divergence_and_plans_explicit_repair() {
        let a = summary(2, 1, 2);
        let b = summary(2, 9, 2);
        let status = detect_fork_or_replay(&a, &b);
        assert!(matches!(status, ForkStatus::Diverged(_)));
        let evidence = match status {
            ForkStatus::Diverged(evidence) => evidence,
            _ => ForkEvidence {
                local: a.clone(),
                remote: b.clone(),
            },
        };
        let plan = plan_repair(
            evidence,
            &[1, 7, 3],
            vec![ApplicationEvent::new(2, 3, "msg", b"ciphertext".to_vec())],
        );
        assert_eq!(
            plan.action,
            RepairAction::RejoinAndReproposal {
                coordinator_leaf: 7
            }
        );
        assert_eq!(plan.winner, b);
        assert_eq!(
            plan.reproposed_events,
            order_application_events(vec![ApplicationEvent::new(
                2,
                3,
                "msg",
                b"ciphertext".to_vec()
            )])
        );
        assert!(!plan.replays_divergent_mls_commits);
        let repaired = repair_to_winner(4, &plan);
        assert_eq!(repaired, vec![plan.winner.clone(); 4]);
        assert!(equal_confirmation_tags(&repaired));
    }

    #[test]
    fn rejects_replay_downgrade_and_forked_commit() {
        let mut state = DeliveryState::new(summary(1, 1, 1));
        let commit = CommitEnvelope::new(
            summary(2, 2, 2),
            1,
            vec![ApplicationEvent::new(2, 1, "m1", b"ciphertext".to_vec())],
        );
        assert_eq!(state.apply_commit(commit), Ok(()));
        assert_eq!(state.accepted_events().len(), 1);
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(1, 1, 1), 1, Vec::new())),
            Err(DeliveryError::DowngradeOrReplay(1))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(2, 2, 2), 1, Vec::new())),
            Err(DeliveryError::DowngradeOrReplay(2))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(summary(2, 9, 2), 1, Vec::new())),
            Err(DeliveryError::DivergentTree(2))
        );
        assert_eq!(
            state.apply_commit(CommitEnvelope::new(
                summary(3, 3, 3),
                1,
                vec![ApplicationEvent::new(2, 1, "old", b"ciphertext".to_vec())],
            )),
            Err(DeliveryError::DowngradeOrReplay(2))
        );
        assert_eq!(state.summary(), &summary(2, 2, 2));
    }

    #[test]
    fn repair_rejoins_winner_and_reproposes_only_current_epoch_events() {
        let local = summary(3, 3, 3);
        let remote = summary(3, 9, 9);
        let evidence = ForkEvidence {
            local: local.clone(),
            remote: remote.clone(),
        };
        let plan = plan_repair(
            evidence,
            &[1, 4],
            vec![
                ApplicationEvent::new(2, 1, "stale", b"old".to_vec()),
                ApplicationEvent::new(3, 4, "valid", b"current".to_vec()),
            ],
        );
        assert_eq!(
            plan.action,
            RepairAction::RejoinAndReproposal {
                coordinator_leaf: 4
            }
        );
        assert_eq!(plan.winner, remote);
        assert_eq!(
            event_ids(&plan.reproposed_events),
            BTreeSet::from(["valid".into()])
        );

        let mut state = DeliveryState::new(local);
        assert_eq!(state.apply_repair_plan(plan), Ok(()));
        assert_eq!(state.summary(), &remote);
        assert_eq!(
            event_ids(&state.accepted_events()),
            BTreeSet::from(["valid".into()])
        );
    }

    #[test]
    fn repair_rejects_divergent_commit_replay_and_stale_targets() {
        let local = summary(4, 4, 4);
        let mut replay_plan = plan_repair(
            ForkEvidence {
                local: local.clone(),
                remote: summary(4, 9, 9),
            },
            &[1],
            Vec::new(),
        );
        replay_plan.replays_divergent_mls_commits = true;
        let mut state = DeliveryState::new(local.clone());
        assert_eq!(
            state.apply_repair_plan(replay_plan),
            Err(DeliveryError::DivergentCommitReplay)
        );

        let stale_plan = RepairPlan {
            action: RepairAction::RejoinAndReproposal {
                coordinator_leaf: 1,
            },
            winner: summary(3, 3, 3),
            reproposed_events: Vec::new(),
            replays_divergent_mls_commits: false,
        };
        assert_eq!(
            state.apply_repair_plan(stale_plan),
            Err(DeliveryError::StaleRepairTarget {
                local: 4,
                target: 3,
            })
        );
        assert_eq!(state.summary(), &local);
    }

    #[test]
    fn welcome_expires_and_catchup_orders_events() {
        let welcome = WelcomePackage::new("room", 4, summary(3, 3, 3), 1_000);
        assert_eq!(welcome.validate(999), Ok(()));
        assert_eq!(welcome.validate(1_001), Err(DeliveryError::WelcomeExpired));
        let catchup = CatchUpBundle::new(
            summary(3, 3, 3),
            Vec::new(),
            vec![
                ApplicationEvent::new(3, 9, "b", b"b".to_vec()),
                ApplicationEvent::new(3, 1, "a", b"a".to_vec()),
            ],
        );
        assert_eq!(catchup.application_events[0].event_id, "a");
    }
}
