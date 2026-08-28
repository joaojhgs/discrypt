//! Broker control lane: sealed, versioned control-frame ferry over rendezvous rooms.
//!
//! The lane carries small, rare coordination frames (MLS admission key packages,
//! MLS Welcomes, governance decisions) over the same signaling provider rooms used
//! for presence and WebRTC negotiation. Every payload is AEAD-sealed with a key
//! derived from invite-shared material, so the provider observes opaque ciphertext
//! and versioned envelope metadata only. Bulk text and media stay on direct
//! peer-to-peer transports; this lane exists so coordination never requires a
//! live DataChannel.

use crate::{
    signaling::{ControlBroadcast, OpaqueSignalingPayload, RendezvousRoom, SignalingPeerId},
    TransportError, WebRtcDataTransportMetrics,
};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use async_trait::async_trait;
use base64::Engine;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::sync::Mutex;

/// Current broker control-lane envelope schema.
pub const BROKER_CONTROL_LANE_SCHEMA_VERSION: u8 = 1;

/// Key-derivation domain separator for the control-lane AEAD key.
pub const BROKER_CONTROL_LANE_KEY_DOMAIN: &[u8] = b"discrypt-broker-control-lane-key-v1";

const BROKER_CONTROL_LANE_LABEL: &str = "broker-control-lane";
const RECV_POLL_INTERVAL: Duration = Duration::from_millis(200);

/// Versioned sealed envelope carried over a rendezvous room's control topic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrokerControlEnvelope {
    /// Envelope schema version.
    pub schema_version: u8,
    /// Sender peer id; receivers skip their own echoed broadcasts.
    pub from_peer: String,
    /// SHA-256 of the plaintext frame, bound as AEAD associated data.
    pub frame_sha256_hex: String,
    /// Base64url AES-GCM nonce.
    pub nonce_b64: String,
    /// Base64url AES-256-GCM ciphertext of the frame bytes.
    pub ciphertext_b64: String,
}

/// Derive the shared AEAD key for a control lane from invite-shared material.
///
/// The material must already be identical on every group member (for example
/// `shared_runtime_material` output) and must never be derivable by the
/// provider from provider-visible fields.
pub fn broker_control_lane_key(shared_material: &[u8]) -> Result<[u8; 32], TransportError> {
    if shared_material.len() < 32 {
        return Err(TransportError::SignalingAdapter(
            "broker control lane key material must be at least 32 bytes".to_owned(),
        ));
    }
    let mut hasher = Sha256::new();
    hasher.update(BROKER_CONTROL_LANE_KEY_DOMAIN);
    hasher.update([0]);
    hasher.update(shared_material);
    let digest: [u8; 32] = hasher.finalize().into();
    Ok(digest)
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn frame_sha256_hex(frame: &[u8]) -> String {
    let digest: [u8; 32] = Sha256::digest(frame).into();
    lower_hex(&digest)
}

fn associated_data(schema_version: u8, from_peer: &str, frame_sha256_hex: &str) -> Vec<u8> {
    let mut aad = Vec::with_capacity(2 + from_peer.len() + frame_sha256_hex.len());
    aad.push(schema_version);
    aad.push(0);
    aad.extend_from_slice(from_peer.as_bytes());
    aad.push(0);
    aad.extend_from_slice(frame_sha256_hex.as_bytes());
    aad
}

/// Seal one control frame into a versioned envelope, serialized for the wire.
pub fn seal_broker_control_frame(
    key: &[u8; 32],
    from_peer: &SignalingPeerId,
    frame_bytes: &[u8],
) -> Result<Vec<u8>, TransportError> {
    if frame_bytes.is_empty() {
        return Err(TransportError::SignalingAdapter(
            "broker control lane frame must not be empty".to_owned(),
        ));
    }
    let frame_sha256_hex = frame_sha256_hex(frame_bytes);
    let mut nonce_bytes = [0_u8; 12];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut nonce_bytes);
    let cipher = Aes256Gcm::new(key.into());
    let ciphertext = cipher
        .encrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: frame_bytes,
                aad: &associated_data(
                    BROKER_CONTROL_LANE_SCHEMA_VERSION,
                    &from_peer.0,
                    &frame_sha256_hex,
                ),
            },
        )
        .map_err(|_| {
            TransportError::SignalingAdapter(
                "broker control lane seal failed".to_owned(),
            )
        })?;
    let encoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let envelope = BrokerControlEnvelope {
        schema_version: BROKER_CONTROL_LANE_SCHEMA_VERSION,
        from_peer: from_peer.0.clone(),
        frame_sha256_hex,
        nonce_b64: encoder.encode(nonce_bytes),
        ciphertext_b64: encoder.encode(ciphertext),
    };
    serde_json::to_vec(&envelope)
        .map_err(|error| TransportError::SignalingAdapter(format!("control envelope encode: {error}")))
}

/// Decode an envelope from wire bytes without opening the ciphertext.
pub fn decode_broker_control_envelope(payload: &[u8]) -> Result<BrokerControlEnvelope, TransportError> {
    let envelope: BrokerControlEnvelope = serde_json::from_slice(payload).map_err(|_| {
        TransportError::SignalingAdapter(
            "broker control lane payload is not a versioned control envelope".to_owned(),
        )
    })?;
    if envelope.schema_version != BROKER_CONTROL_LANE_SCHEMA_VERSION {
        return Err(TransportError::SignalingAdapter(format!(
            "broker control lane envelope schema {} is unsupported",
            envelope.schema_version
        )));
    }
    Ok(envelope)
}

/// Open a decoded envelope and return the plaintext frame bytes.
pub fn open_broker_control_frame(
    key: &[u8; 32],
    envelope: &BrokerControlEnvelope,
) -> Result<Vec<u8>, TransportError> {
    let encoder = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let nonce_bytes = encoder
        .decode(envelope.nonce_b64.as_bytes())
        .ok()
        .and_then(|bytes| <[u8; 12]>::try_from(bytes).ok())
        .ok_or_else(|| {
            TransportError::SignalingAdapter(
                "broker control lane envelope nonce is invalid".to_owned(),
            )
        })?;
    let ciphertext = encoder
        .decode(envelope.ciphertext_b64.as_bytes())
        .map_err(|_| {
            TransportError::SignalingAdapter(
                "broker control lane envelope ciphertext is invalid".to_owned(),
            )
        })?;
    let cipher = Aes256Gcm::new(key.into());
    let frame_bytes = cipher
        .decrypt(
            Nonce::from_slice(&nonce_bytes),
            Payload {
                msg: &ciphertext,
                aad: &associated_data(
                    envelope.schema_version,
                    &envelope.from_peer,
                    &envelope.frame_sha256_hex,
                ),
            },
        )
        .map_err(|_| {
            TransportError::SignalingAdapter(
                "broker control lane envelope failed authentication".to_owned(),
            )
        })?;
    let actual_sha256 = frame_sha256_hex(&frame_bytes);
    if actual_sha256 != envelope.frame_sha256_hex {
        return Err(TransportError::SignalingAdapter(
            "broker control lane envelope frame digest mismatch".to_owned(),
        ));
    }
    Ok(frame_bytes)
}

/// [`TextControlDataTransport`] implementation that ferries sealed control frames
/// over a joined rendezvous room's control topic.
pub struct BrokerControlLaneTransport {
    room: Box<dyn RendezvousRoom>,
    key: [u8; 32],
    local_peer_id: SignalingPeerId,
    inbound: Mutex<VecDeque<Vec<u8>>>,
    frames_sent: AtomicU64,
    frames_received: AtomicU64,
}

impl BrokerControlLaneTransport {
    /// Wrap a joined rendezvous room as a text/control data transport.
    pub fn new(
        room: Box<dyn RendezvousRoom>,
        key: [u8; 32],
        local_peer_id: SignalingPeerId,
    ) -> Self {
        Self {
            room,
            key,
            local_peer_id,
            inbound: Mutex::new(VecDeque::new()),
            frames_sent: AtomicU64::new(0),
            frames_received: AtomicU64::new(0),
        }
    }

    async fn take_remote_frames(&self) -> Result<Vec<Vec<u8>>, TransportError> {
        let broadcasts: Vec<ControlBroadcast> = self.room.take_control_payloads().await?;
        let mut opened = Vec::with_capacity(broadcasts.len());
        for broadcast in broadcasts {
            if broadcast.from_peer == self.local_peer_id {
                continue;
            }
            let envelope = decode_broker_control_envelope(&broadcast.payload.bytes)?;
            opened.push(open_broker_control_frame(&self.key, &envelope)?);
        }
        Ok(opened)
    }
}

#[async_trait]
impl crate::TextControlDataTransport for BrokerControlLaneTransport {
    async fn send_text_control_frame(&self, frame: Vec<u8>) -> Result<(), TransportError> {
        let payload = seal_broker_control_frame(&self.key, &self.local_peer_id, &frame)?;
        self.room
            .broadcast_control(OpaqueSignalingPayload::new(payload)?)
            .await?;
        self.frames_sent.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    async fn recv_text_control_frame(&self) -> Result<Vec<u8>, TransportError> {
        loop {
            if let Some(frame) = self.inbound.lock().await.pop_front() {
                return Ok(frame);
            }
            let frames = self.take_remote_frames().await?;
            if !frames.is_empty() {
                let mut inbound = self.inbound.lock().await;
                inbound.extend(frames);
                if let Some(frame) = inbound.pop_front() {
                    self.frames_received.fetch_add(1, Ordering::Relaxed);
                    return Ok(frame);
                }
            }
            tokio::time::sleep(RECV_POLL_INTERVAL).await;
        }
    }

    async fn text_control_transport_metrics(&self) -> WebRtcDataTransportMetrics {
        WebRtcDataTransportMetrics {
            schema_version: WebRtcDataTransportMetrics::SCHEMA_VERSION,
            label: BROKER_CONTROL_LANE_LABEL.to_owned(),
            attached_channels: 1,
            open: true,
            frames_sent: self.frames_sent.load(Ordering::Relaxed),
            frames_received: self.frames_received.load(Ordering::Relaxed),
            bytes_sent: 0,
            bytes_received: 0,
            last_state: "open".to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used)]

    use super::*;

    fn test_key() -> [u8; 32] {
        broker_control_lane_key(&[7_u8; 32]).expect("32-byte material derives a key")
    }

    #[test]
    fn control_lane_key_requires_sufficient_material() {
        assert!(broker_control_lane_key(&[1_u8; 8]).is_err());
        assert!(broker_control_lane_key(&[1_u8; 32]).is_ok());
    }

    #[test]
    fn control_lane_key_is_deterministic_and_domain_separated() {
        let first = broker_control_lane_key(&[9_u8; 64]).expect("key");
        let second = broker_control_lane_key(&[9_u8; 64]).expect("key");
        let other = broker_control_lane_key(&[8_u8; 64]).expect("key");
        assert_eq!(first, second);
        assert_ne!(first, other);
    }

    #[test]
    fn seal_open_roundtrip_restores_frame_bytes() {
        let key = test_key();
        let peer = SignalingPeerId::new("peer-alice".to_owned()).expect("peer id");
        let frame = b"{\"kind\":\"test-frame\"}".to_vec();
        let wire = seal_broker_control_frame(&key, &peer, &frame).expect("seal");
        let envelope = decode_broker_control_envelope(&wire).expect("decode");
        assert_eq!(envelope.from_peer, "peer-alice");
        assert_eq!(
            envelope.schema_version,
            BROKER_CONTROL_LANE_SCHEMA_VERSION
        );
        let opened = open_broker_control_frame(&key, &envelope).expect("open");
        assert_eq!(opened, frame);
    }

    #[test]
    fn tampered_envelope_fails_authentication() {
        let key = test_key();
        let peer = SignalingPeerId::new("peer-alice".to_owned()).expect("peer id");
        let wire = seal_broker_control_frame(&key, &peer, b"frame-bytes").expect("seal");
        let mut envelope = decode_broker_control_envelope(&wire).expect("decode");
        let mut ciphertext =
            base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(envelope.ciphertext_b64.as_bytes())
                .expect("ciphertext decode");
        ciphertext[0] ^= 1;
        envelope.ciphertext_b64 =
            base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&ciphertext);
        assert!(open_broker_control_frame(&key, &envelope).is_err());
    }

    #[test]
    fn wrong_key_fails_authentication() {
        let peer = SignalingPeerId::new("peer-alice".to_owned()).expect("peer id");
        let wire = seal_broker_control_frame(&test_key(), &peer, b"frame-bytes").expect("seal");
        let envelope = decode_broker_control_envelope(&wire).expect("decode");
        let other_key = broker_control_lane_key(&[11_u8; 32]).expect("other key");
        assert!(open_broker_control_frame(&other_key, &envelope).is_err());
    }

    #[test]
    fn digest_mismatch_fails_closed() {
        let key = test_key();
        let peer = SignalingPeerId::new("peer-alice".to_owned()).expect("peer id");
        let wire = seal_broker_control_frame(&key, &peer, b"frame-bytes").expect("seal");
        let mut envelope = decode_broker_control_envelope(&wire).expect("decode");
        envelope.frame_sha256_hex = "0".repeat(64);
        assert!(open_broker_control_frame(&key, &envelope).is_err());
    }
}
