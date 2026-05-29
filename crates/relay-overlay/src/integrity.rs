//! Relay integrity helpers for media bytes.
//!
//! Relays are intentionally content-blind: they may forward, drop, replay, or
//! corrupt bytes, but media authentication and anti-replay are receiver-owned.
use thiserror::Error;

use crate::topology::MAX_RELAY_HOPS;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Relay integrity errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RelayIntegrityError {
    /// Forwarding would exceed the bounded relay depth.
    #[error("relay hop limit exceeded")]
    HopLimitExceeded,
    /// Relays are not allowed to forward empty ciphertext frames.
    #[error("relay packet has empty ciphertext")]
    EmptyCiphertext,
    /// Relay-visible payloads must carry KID/counter/AAD metadata.
    #[error("relay packet is missing protected-frame metadata")]
    MissingProtectedMetadata,
}

/// Relay-visible protected payload class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RelayPayloadKind {
    /// Protected voice/media frame.
    Media,
    /// Protected text/control frame.
    TextControl,
    /// Protected store-forward envelope.
    StoreForward,
}

/// Relay-visible protected frame/envelope.
///
/// Relays may inspect routing metadata plus this KID/counter/AAD commitment, but
/// never receive a bare ciphertext byte slice detached from replay and
/// authentication context.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayProtectedEnvelope {
    /// Payload class for policy/routing decisions.
    pub kind: RelayPayloadKind,
    /// Key identifier from the protected frame header.
    pub kid: Vec<u8>,
    /// Sender monotonic frame/message counter.
    pub counter: u64,
    /// Commitment to the authenticated associated data, not the raw AAD.
    pub aad_commitment: [u8; 32],
    /// Encrypted/authenticated bytes.
    pub ciphertext: Vec<u8>,
}

impl RelayProtectedEnvelope {
    /// Build a relay-visible protected envelope from frame metadata.
    pub fn new(
        kind: RelayPayloadKind,
        kid: Vec<u8>,
        counter: u64,
        aad_metadata: &[u8],
        ciphertext: Vec<u8>,
    ) -> Result<Self, RelayIntegrityError> {
        if kid.is_empty() || aad_metadata.is_empty() {
            return Err(RelayIntegrityError::MissingProtectedMetadata);
        }
        if ciphertext.is_empty() {
            return Err(RelayIntegrityError::EmptyCiphertext);
        }
        let aad_commitment = aad_commitment(kind, &kid, counter, aad_metadata);
        Ok(Self {
            kind,
            kid,
            counter,
            aad_commitment,
            ciphertext,
        })
    }

    /// Relay-visible bytes used by audits. This includes only public metadata
    /// and ciphertext, never raw AAD or plaintext.
    #[must_use]
    pub fn visible_bytes(&self) -> Vec<u8> {
        let mut visible = Vec::with_capacity(
            self.kid.len() + self.aad_commitment.len() + self.ciphertext.len() + 16,
        );
        visible.extend_from_slice(match self.kind {
            RelayPayloadKind::Media => b"media",
            RelayPayloadKind::TextControl => b"text_control",
            RelayPayloadKind::StoreForward => b"store_forward",
        });
        visible.extend_from_slice(&(self.kid.len() as u64).to_be_bytes());
        visible.extend_from_slice(&self.kid);
        visible.extend_from_slice(&self.counter.to_be_bytes());
        visible.extend_from_slice(&self.aad_commitment);
        visible.extend_from_slice(&self.ciphertext);
        visible
    }

    /// Validate the envelope has the replay/authentication metadata relays must carry.
    pub fn validate(&self) -> Result<(), RelayIntegrityError> {
        if self.kid.is_empty() {
            return Err(RelayIntegrityError::MissingProtectedMetadata);
        }
        if self.ciphertext.is_empty() {
            return Err(RelayIntegrityError::EmptyCiphertext);
        }
        Ok(())
    }
}

/// Relay-visible protected envelope plus routing metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayPacket {
    /// Opaque route or peer id used by the overlay.
    pub next_hop: String,
    /// Protected frame/envelope with KID, counter, AAD commitment, and ciphertext.
    pub envelope: RelayProtectedEnvelope,
    /// Number of content-blind relay hops already traversed.
    pub hop_count: u8,
}

impl RelayPacket {
    /// Build a content-blind relay packet from a protected envelope.
    #[must_use]
    pub fn from_envelope(next_hop: impl Into<String>, envelope: RelayProtectedEnvelope) -> Self {
        Self {
            next_hop: next_hop.into(),
            envelope,
            hop_count: 0,
        }
    }

    /// Validate that the packet is relay-eligible ciphertext.
    pub fn validate_ciphertext_only(&self) -> Result<(), RelayIntegrityError> {
        self.envelope.validate()?;
        if usize::from(self.hop_count) > MAX_RELAY_HOPS {
            return Err(RelayIntegrityError::HopLimitExceeded);
        }
        Ok(())
    }

    /// Forward unchanged bytes to the next hop without exposing content.
    #[must_use]
    pub fn forward(self, next_hop: impl Into<String>) -> Self {
        Self {
            next_hop: next_hop.into(),
            envelope: self.envelope,
            hop_count: self.hop_count.saturating_add(1),
        }
    }

    /// Forward unchanged bytes only if the hop limit remains satisfied.
    pub fn forward_checked(self, next_hop: impl Into<String>) -> Result<Self, RelayIntegrityError> {
        self.validate_ciphertext_only()?;
        let forwarded = self.forward(next_hop);
        forwarded.validate_ciphertext_only()?;
        Ok(forwarded)
    }

    /// Simulate an active relay bit flip.
    #[must_use]
    pub fn tamper(mut self) -> Self {
        if let Some(first) = self.envelope.ciphertext.first_mut() {
            *first ^= 0x80;
        }
        self
    }
}

/// Check whether relay bytes visibly contain a plaintext window.
#[must_use]
pub fn contains_plaintext(packet: &RelayPacket, plaintext: &[u8]) -> bool {
    !plaintext.is_empty()
        && packet
            .envelope
            .visible_bytes()
            .windows(plaintext.len())
            .any(|window| window == plaintext)
}

fn aad_commitment(
    kind: RelayPayloadKind,
    kid: &[u8],
    counter: u64,
    aad_metadata: &[u8],
) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-relay-protected-aad-v1");
    h.update(match kind {
        RelayPayloadKind::Media => b"media".as_slice(),
        RelayPayloadKind::TextControl => b"text_control".as_slice(),
        RelayPayloadKind::StoreForward => b"store_forward".as_slice(),
    });
    h.update((kid.len() as u64).to_be_bytes());
    h.update(kid);
    h.update(counter.to_be_bytes());
    h.update((aad_metadata.len() as u64).to_be_bytes());
    h.update(aad_metadata);
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(ciphertext: &[u8]) -> Result<RelayProtectedEnvelope, RelayIntegrityError> {
        RelayProtectedEnvelope::new(
            RelayPayloadKind::Media,
            b"kid".to_vec(),
            7,
            b"route:a:b",
            ciphertext.to_vec(),
        )
    }

    #[test]
    fn relay_forwarding_preserves_ciphertext_opacity() -> Result<(), RelayIntegrityError> {
        let packet = RelayPacket::from_envelope("relay-a", envelope(b"DCF1\x01ciphertext-only")?);
        let forwarded = packet.forward_checked("relay-b")?;
        assert_eq!(forwarded.next_hop, "relay-b");
        assert_eq!(forwarded.hop_count, 1);
        assert!(!contains_plaintext(&forwarded, b"voice-frame"));
        Ok(())
    }

    #[test]
    fn active_relay_tamper_changes_bytes_for_receiver_detection() -> Result<(), RelayIntegrityError>
    {
        let packet = RelayPacket::from_envelope("relay-a", envelope(b"DCF1\x01ciphertext-only")?);
        let tampered = packet.clone().tamper();
        assert_ne!(packet.envelope.ciphertext, tampered.envelope.ciphertext);
        Ok(())
    }

    #[test]
    fn checked_forward_rejects_excess_hops_and_empty_ciphertext() -> Result<(), RelayIntegrityError>
    {
        let mut packet = RelayPacket::from_envelope("relay-a", envelope(b"ciphertext")?);
        packet.hop_count = MAX_RELAY_HOPS as u8;
        assert_eq!(
            packet.forward_checked("relay-b"),
            Err(RelayIntegrityError::HopLimitExceeded)
        );
        assert_eq!(
            RelayProtectedEnvelope::new(
                RelayPayloadKind::Media,
                b"kid".to_vec(),
                1,
                b"aad",
                Vec::new()
            ),
            Err(RelayIntegrityError::EmptyCiphertext)
        );
        Ok(())
    }

    #[test]
    fn rejects_raw_ciphertext_without_kid_counter_and_aad_metadata() {
        assert_eq!(
            RelayProtectedEnvelope::new(
                RelayPayloadKind::Media,
                Vec::new(),
                1,
                b"route",
                b"ciphertext".to_vec()
            ),
            Err(RelayIntegrityError::MissingProtectedMetadata)
        );
        assert_eq!(
            RelayProtectedEnvelope::new(
                RelayPayloadKind::Media,
                b"kid".to_vec(),
                1,
                b"",
                b"ciphertext".to_vec()
            ),
            Err(RelayIntegrityError::MissingProtectedMetadata)
        );
    }
}
