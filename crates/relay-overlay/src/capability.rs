//! Content-blind peer capability advertisements for relay selection.
//!
//! Advertisements are emitted by authenticated overlay peers and carry only
//! routing health/capacity metadata: relay capacity, battery/doze posture,
//! observed RTT, packet loss, and freeload accounting. They intentionally do not
//! carry group ids, message ids, plaintext, content keys, or media metadata.

use crate::manager::RelayRuntimeObservation;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

/// Local battery/doze posture advertised for relay ranking.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BatteryDozePosture {
    /// Device is charging or externally powered.
    Charging,
    /// Device is on battery but not constrained.
    BatteryNormal,
    /// Device has entered an OS battery-saver posture.
    BatterySaver,
    /// Device is in doze/background constrained mode.
    Dozing,
    /// Caller cannot classify the current posture.
    Unknown,
}

impl BatteryDozePosture {
    /// Convert posture into a ranking cost in basis points.
    #[must_use]
    pub const fn cost_bps(self) -> u16 {
        match self {
            Self::Charging => 0,
            Self::BatteryNormal => 250,
            Self::Unknown => 1_000,
            Self::BatterySaver => 2_500,
            Self::Dozing => 7_500,
        }
    }
}

/// Relay capacity advertised by a peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayCapacityAdvertisement {
    /// Maximum relay fanout the peer is willing to serve now.
    pub max_fanout: u16,
    /// Current relay egress budget in bytes per second.
    pub egress_bytes_per_second: u64,
    /// Whether this peer accepts opportunistic store-forward envelopes.
    pub accepts_store_forward: bool,
}

/// One signed/transport-authenticated capability advertisement payload.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RelayCapabilityAdvertisement {
    /// Stable authenticated peer id.
    pub peer_id: String,
    /// Monotonic sender sequence used to reject stale advertisements.
    pub sequence: u64,
    /// Sender wall-clock or monotonic timestamp in milliseconds.
    pub issued_at_ms: u64,
    /// Expiry timestamp in milliseconds; stale advertisements are ignored.
    pub expires_at_ms: u64,
    /// Current relay capacity posture.
    pub relay_capacity: RelayCapacityAdvertisement,
    /// Current battery/doze posture.
    pub battery_doze: BatteryDozePosture,
    /// Observed RTT from the local peer to this relay candidate.
    pub observed_rtt_ms: u32,
    /// Packet loss over the advertisement window in basis points.
    pub packet_loss_bps: u16,
    /// Relay bytes this peer contributed in the current accounting window.
    pub contributed_bytes: u64,
    /// Relay bytes this peer consumed in the current accounting window.
    pub consumed_bytes: u64,
}

impl RelayCapabilityAdvertisement {
    /// Maximum basis-points value.
    pub const MAX_BPS: u16 = 10_000;

    /// Validate freshness and non-placeholder routing metrics.
    pub fn validate_at(&self, now_ms: u64) -> Result<(), CapabilityAdvertisementError> {
        if self.peer_id.trim().is_empty() {
            return Err(CapabilityAdvertisementError::Invalid(
                "peer id is required".to_owned(),
            ));
        }
        if self.sequence == 0 {
            return Err(CapabilityAdvertisementError::Invalid(
                "sequence must be non-zero".to_owned(),
            ));
        }
        if self.expires_at_ms <= now_ms || self.expires_at_ms <= self.issued_at_ms {
            return Err(CapabilityAdvertisementError::Expired);
        }
        if self.relay_capacity.max_fanout == 0 || self.relay_capacity.egress_bytes_per_second == 0 {
            return Err(CapabilityAdvertisementError::Invalid(
                "relay capacity must include positive fanout and egress budget".to_owned(),
            ));
        }
        if self.packet_loss_bps > Self::MAX_BPS {
            return Err(CapabilityAdvertisementError::Invalid(
                "packet loss must be basis points".to_owned(),
            ));
        }
        Ok(())
    }

    /// Contribution deficit score in basis points. Higher is worse.
    #[must_use]
    pub fn freeload_score_bps(&self) -> u16 {
        if self.consumed_bytes == 0 {
            return 0;
        }
        if self.contributed_bytes >= self.consumed_bytes {
            return 0;
        }
        let deficit = self.consumed_bytes.saturating_sub(self.contributed_bytes) as u128;
        let consumed = self.consumed_bytes.max(1) as u128;
        ((deficit.saturating_mul(u128::from(Self::MAX_BPS))) / consumed)
            .min(u128::from(Self::MAX_BPS)) as u16
    }

    /// Canonical content-blind digest used for logs and replay diagnostics.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.peer_id.as_bytes());
        hasher.update(self.sequence.to_be_bytes());
        hasher.update(self.issued_at_ms.to_be_bytes());
        hasher.update(self.expires_at_ms.to_be_bytes());
        hasher.update(self.relay_capacity.max_fanout.to_be_bytes());
        hasher.update(self.relay_capacity.egress_bytes_per_second.to_be_bytes());
        hasher.update([u8::from(self.relay_capacity.accepts_store_forward)]);
        hasher.update((self.battery_doze.cost_bps()).to_be_bytes());
        hasher.update(self.observed_rtt_ms.to_be_bytes());
        hasher.update(self.packet_loss_bps.to_be_bytes());
        hasher.update(self.freeload_score_bps().to_be_bytes());
        hasher.finalize().into()
    }

    /// Convert this advertisement into the runtime observation used by [`crate::OverlayManager`].
    #[must_use]
    pub fn to_runtime_observation(&self) -> RelayRuntimeObservation {
        let successful_probes = u32::from(Self::MAX_BPS.saturating_sub(self.packet_loss_bps));
        let failed_probes = u32::from(self.packet_loss_bps);
        RelayRuntimeObservation {
            peer_id: self.peer_id.clone(),
            latency_ms: self.observed_rtt_ms.max(1),
            successful_probes,
            failed_probes,
            battery_cost_bps: self.battery_doze.cost_bps(),
            contributed_bytes: self.contributed_bytes,
            consumed_bytes: self.consumed_bytes,
        }
    }
}

/// Errors returned by capability advertisement validation/storage.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum CapabilityAdvertisementError {
    /// Advertisement failed structural validation.
    #[error("invalid relay capability advertisement: {0}")]
    Invalid(String),
    /// Advertisement has expired.
    #[error("relay capability advertisement expired")]
    Expired,
    /// Advertisement sequence does not advance the latest known peer state.
    #[error("stale relay capability advertisement for {peer_id}: {sequence} <= {latest}")]
    Stale {
        /// Peer whose advertisement was stale.
        peer_id: String,
        /// Rejected sequence.
        sequence: u64,
        /// Latest accepted sequence.
        latest: u64,
    },
}

/// Latest accepted capability advertisements keyed by peer id.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CapabilityAdvertisementBook {
    latest: BTreeMap<String, RelayCapabilityAdvertisement>,
}

impl CapabilityAdvertisementBook {
    /// Accept a fresh advertisement and reject expired/stale updates.
    pub fn accept(
        &mut self,
        advertisement: RelayCapabilityAdvertisement,
        now_ms: u64,
    ) -> Result<(), CapabilityAdvertisementError> {
        advertisement.validate_at(now_ms)?;
        if let Some(latest) = self.latest.get(&advertisement.peer_id) {
            if advertisement.sequence <= latest.sequence {
                return Err(CapabilityAdvertisementError::Stale {
                    peer_id: advertisement.peer_id,
                    sequence: advertisement.sequence,
                    latest: latest.sequence,
                });
            }
        }
        self.latest
            .insert(advertisement.peer_id.clone(), advertisement);
        Ok(())
    }

    /// Return latest advertisement for a peer.
    #[must_use]
    pub fn get(&self, peer_id: &str) -> Option<&RelayCapabilityAdvertisement> {
        self.latest.get(peer_id)
    }

    /// Number of latest advertisements retained.
    #[must_use]
    pub fn len(&self) -> usize {
        self.latest.len()
    }

    /// True when no advertisements are retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.latest.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ad(peer_id: &str, sequence: u64) -> RelayCapabilityAdvertisement {
        RelayCapabilityAdvertisement {
            peer_id: peer_id.to_owned(),
            sequence,
            issued_at_ms: 1_000,
            expires_at_ms: 2_000,
            relay_capacity: RelayCapacityAdvertisement {
                max_fanout: 8,
                egress_bytes_per_second: 256_000,
                accepts_store_forward: true,
            },
            battery_doze: BatteryDozePosture::BatteryNormal,
            observed_rtt_ms: 42,
            packet_loss_bps: 125,
            contributed_bytes: 10_000,
            consumed_bytes: 40_000,
        }
    }

    #[test]
    fn advertisement_validates_and_converts_to_runtime_observation(
    ) -> Result<(), CapabilityAdvertisementError> {
        let advertisement = ad("relay-a", 1);
        advertisement.validate_at(1_500)?;
        assert_eq!(advertisement.freeload_score_bps(), 7_500);
        let observation = advertisement.to_runtime_observation();
        assert_eq!(observation.peer_id, "relay-a");
        assert_eq!(observation.latency_ms, 42);
        assert_eq!(observation.successful_probes, 9_875);
        assert_eq!(observation.failed_probes, 125);
        assert_eq!(observation.battery_cost_bps, 250);
        assert_ne!(advertisement.digest(), [0u8; 32]);
        Ok(())
    }

    #[test]
    fn advertisement_book_rejects_expired_and_stale_updates(
    ) -> Result<(), CapabilityAdvertisementError> {
        let mut book = CapabilityAdvertisementBook::default();
        assert_eq!(book.accept(ad("relay-a", 1), 1_500), Ok(()));
        assert_eq!(book.len(), 1);
        assert!(matches!(
            book.accept(ad("relay-a", 1), 1_500),
            Err(CapabilityAdvertisementError::Stale { .. })
        ));
        assert!(matches!(
            book.accept(ad("relay-b", 1), 2_000),
            Err(CapabilityAdvertisementError::Expired)
        ));
        assert_eq!(book.accept(ad("relay-a", 2), 1_500), Ok(()));
        assert_eq!(book.get("relay-a").map(|ad| ad.sequence), Some(2));
        Ok(())
    }

    #[test]
    fn rejects_placeholder_capacity_and_invalid_loss() {
        let mut invalid = ad("relay", 1);
        invalid.relay_capacity.max_fanout = 0;
        assert!(matches!(
            invalid.validate_at(1_500),
            Err(CapabilityAdvertisementError::Invalid(_))
        ));
        invalid = ad("relay", 1);
        invalid.packet_loss_bps = RelayCapabilityAdvertisement::MAX_BPS + 1;
        assert!(matches!(
            invalid.validate_at(1_500),
            Err(CapabilityAdvertisementError::Invalid(_))
        ));
    }
}
