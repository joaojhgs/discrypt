//! Headless multinode harness for discrypt acceptance tests.
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
    /// Receiver plaintext after successful authentication and replay acceptance.
    pub plaintext: Vec<u8>,
}

/// Exercise passive relay opacity, active tamper rejection, and anti-replay checks.
pub fn media_security_smoke() -> Result<MediaSecuritySmoke, discrypt_media::MediaError> {
    use discrypt_media::{
        MediaKeyRegistry, ReplayWindow, SFrameReceiver, SFrameSender, SenderBinding,
    };
    use discrypt_relay_overlay::integrity::{contains_plaintext, RelayPacket};

    let binding = SenderBinding {
        kid: b"harness-kid-alice".to_vec(),
        leaf_index: 1,
        device_id: "alice-laptop".to_owned(),
    };
    let mut sender = SFrameSender::new(&[9; 32], binding.clone())?;
    let mut registry = MediaKeyRegistry::new();
    registry.register_sender(&[9; 32], binding.clone())?;
    let mut tamper_registry = MediaKeyRegistry::new();
    tamper_registry.register_sender(&[9; 32], binding)?;
    let mut receiver = SFrameReceiver::new(registry, ReplayWindow::default());

    let plaintext = b"harness encoded voice frame";
    let relayed = sender.protect(plaintext)?;
    let relay_packet = RelayPacket::new("relay-a", relayed.ciphertext.clone()).forward("relay-b");
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

    Ok(MediaSecuritySmoke {
        passive_relay_cannot_read,
        replay_rejected,
        tamper_rejected,
        plaintext: opened.plaintext,
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
                plaintext
            }) if plaintext == b"harness encoded voice frame"
        ));
    }
}
