//! Relay integrity helpers for media bytes.
//!
//! Relays are intentionally content-blind: they may forward, drop, replay, or
//! corrupt bytes, but media authentication and anti-replay are receiver-owned.
use thiserror::Error;

use crate::topology::MAX_RELAY_HOPS;

/// Relay integrity errors.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum RelayIntegrityError {
    /// Forwarding would exceed the bounded relay depth.
    #[error("relay hop limit exceeded")]
    HopLimitExceeded,
    /// Relays are not allowed to forward empty ciphertext frames.
    #[error("relay packet has empty ciphertext")]
    EmptyCiphertext,
}

/// Relay-visible packet bytes plus routing metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayPacket {
    /// Opaque route or peer id used by the overlay.
    pub next_hop: String,
    /// Encrypted/authenticated media frame bytes.
    pub bytes: Vec<u8>,
    /// Number of content-blind relay hops already traversed.
    pub hop_count: u8,
}

impl RelayPacket {
    /// Build a content-blind relay packet.
    #[must_use]
    pub fn new(next_hop: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            next_hop: next_hop.into(),
            bytes,
            hop_count: 0,
        }
    }

    /// Validate that the packet is relay-eligible ciphertext.
    pub fn validate_ciphertext_only(&self) -> Result<(), RelayIntegrityError> {
        if self.bytes.is_empty() {
            return Err(RelayIntegrityError::EmptyCiphertext);
        }
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
            bytes: self.bytes,
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
        if let Some(first) = self.bytes.first_mut() {
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
            .bytes
            .windows(plaintext.len())
            .any(|window| window == plaintext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relay_forwarding_preserves_ciphertext_opacity() -> Result<(), RelayIntegrityError> {
        let packet = RelayPacket::new("relay-a", b"DCF1\x01ciphertext-only".to_vec());
        let forwarded = packet.forward_checked("relay-b")?;
        assert_eq!(forwarded.next_hop, "relay-b");
        assert_eq!(forwarded.hop_count, 1);
        assert!(!contains_plaintext(&forwarded, b"voice-frame"));
        Ok(())
    }

    #[test]
    fn active_relay_tamper_changes_bytes_for_receiver_detection() {
        let packet = RelayPacket::new("relay-a", b"DCF1\x01ciphertext-only".to_vec());
        let tampered = packet.clone().tamper();
        assert_ne!(packet.bytes, tampered.bytes);
    }

    #[test]
    fn checked_forward_rejects_excess_hops_and_empty_ciphertext() {
        let mut packet = RelayPacket::new("relay-a", b"ciphertext".to_vec());
        packet.hop_count = MAX_RELAY_HOPS as u8;
        assert_eq!(
            packet.forward_checked("relay-b"),
            Err(RelayIntegrityError::HopLimitExceeded)
        );
        assert_eq!(
            RelayPacket::new("relay-a", Vec::new()).validate_ciphertext_only(),
            Err(RelayIntegrityError::EmptyCiphertext)
        );
    }
}
