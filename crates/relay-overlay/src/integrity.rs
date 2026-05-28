//! Relay integrity helpers for media bytes.
//!
//! Relays are intentionally content-blind: they may forward, drop, replay, or
//! corrupt bytes, but media authentication and anti-replay are receiver-owned.

/// Relay-visible packet bytes plus routing metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RelayPacket {
    /// Opaque route or peer id used by the overlay.
    pub next_hop: String,
    /// Encrypted/authenticated media frame bytes.
    pub bytes: Vec<u8>,
}

impl RelayPacket {
    /// Build a content-blind relay packet.
    #[must_use]
    pub fn new(next_hop: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            next_hop: next_hop.into(),
            bytes,
        }
    }

    /// Forward unchanged bytes to the next hop.
    #[must_use]
    pub fn forward(self, next_hop: impl Into<String>) -> Self {
        Self {
            next_hop: next_hop.into(),
            bytes: self.bytes,
        }
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
    fn relay_forwarding_preserves_ciphertext_opacity() {
        let packet = RelayPacket::new("relay-a", b"DCF1\x01ciphertext-only".to_vec());
        let forwarded = packet.forward("relay-b");
        assert_eq!(forwarded.next_hop, "relay-b");
        assert!(!contains_plaintext(&forwarded, b"voice-frame"));
    }

    #[test]
    fn active_relay_tamper_changes_bytes_for_receiver_detection() {
        let packet = RelayPacket::new("relay-a", b"DCF1\x01ciphertext-only".to_vec());
        let tampered = packet.clone().tamper();
        assert_ne!(packet.bytes, tampered.bytes);
    }
}
