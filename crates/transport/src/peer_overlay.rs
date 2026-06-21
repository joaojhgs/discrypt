//! Peer-assisted encrypted overlay protocol contract.
//!
//! This module is intentionally data-only. It validates the shape of future
//! relay frames without selecting routes, authorizing relay candidates,
//! forwarding bytes, or exposing any decrypt/key path.

use crate::{SignalingPeerId, TransportError};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Serializable schema version for peer overlay protocol frames.
pub const PEER_OVERLAY_FRAME_SCHEMA_VERSION: u16 = 1;

/// Maximum peer-assisted relay hops allowed by the product plan.
pub const PEER_OVERLAY_MAX_RELAY_HOPS: u8 = 3;

/// Opaque loop id used to suppress relay loops and duplicate attempts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PeerOverlayLoopId(pub [u8; 16]);

impl PeerOverlayLoopId {
    /// Validate that the loop id can identify a concrete send attempt.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.0 == [0; 16] {
            Err(overlay_policy_error(
                "peer overlay loop id must not be all zero",
            ))
        } else {
            Ok(())
        }
    }
}

/// Opaque ack id used to correlate receiver acknowledgements and redelivery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PeerOverlayAckId(pub [u8; 16]);

impl PeerOverlayAckId {
    /// Validate that the ack id can identify a concrete delivery attempt.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.0 == [0; 16] {
            Err(overlay_policy_error(
                "peer overlay ack id must not be all zero",
            ))
        } else {
            Ok(())
        }
    }
}

/// Peer/device reference proven by backend/OpenMLS membership state.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct PeerOverlayPeerRef {
    /// Transport-level authenticated peer id.
    pub peer_id: SignalingPeerId,
    /// Backend/OpenMLS-governed member id.
    pub member_id: String,
    /// Backend/OpenMLS-governed device id.
    pub device_id: String,
    /// Group epoch where this peer is admitted.
    pub epoch: u64,
}

impl PeerOverlayPeerRef {
    /// Build a peer ref from admitted backend/OpenMLS state.
    #[must_use]
    pub fn new(
        peer_id: SignalingPeerId,
        member_id: impl Into<String>,
        device_id: impl Into<String>,
        epoch: u64,
    ) -> Self {
        Self {
            peer_id,
            member_id: member_id.into(),
            device_id: device_id.into(),
            epoch,
        }
    }

    fn validate_shape(&self) -> Result<(), TransportError> {
        validate_label(&self.member_id, "peer overlay member id")?;
        validate_label(&self.device_id, "peer overlay device id")?;
        Ok(())
    }
}

/// Current admitted/revoked peer set supplied by backend/OpenMLS state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayAdmittedSet {
    /// Current group epoch for the frame validator.
    pub current_epoch: u64,
    admitted: BTreeMap<SignalingPeerId, PeerOverlayPeerRef>,
    revoked: BTreeSet<SignalingPeerId>,
}

impl PeerOverlayAdmittedSet {
    /// Build a peer set from admitted current-epoch refs and revoked peer ids.
    pub fn new(
        current_epoch: u64,
        admitted: impl IntoIterator<Item = PeerOverlayPeerRef>,
        revoked: impl IntoIterator<Item = SignalingPeerId>,
    ) -> Result<Self, TransportError> {
        let revoked = revoked.into_iter().collect::<BTreeSet<_>>();
        let mut admitted_map = BTreeMap::new();
        for peer in admitted {
            peer.validate_shape()?;
            if peer.epoch != current_epoch {
                return Err(overlay_policy_error(
                    "peer overlay admitted peer epoch must match current epoch",
                ));
            }
            if revoked.contains(&peer.peer_id) {
                return Err(overlay_policy_error(
                    "peer overlay admitted set must not include revoked peers",
                ));
            }
            if admitted_map.insert(peer.peer_id.clone(), peer).is_some() {
                return Err(overlay_policy_error(
                    "peer overlay admitted peers must be unique",
                ));
            }
        }
        if admitted_map.len() < 2 {
            return Err(overlay_policy_error(
                "peer overlay requires at least two admitted peers",
            ));
        }
        Ok(Self {
            current_epoch,
            admitted: admitted_map,
            revoked,
        })
    }

    /// Validate one frame ref against current admitted/revoked state.
    pub fn validate_ref(&self, peer: &PeerOverlayPeerRef) -> Result<(), TransportError> {
        peer.validate_shape()?;
        if peer.epoch != self.current_epoch {
            return Err(overlay_policy_error(
                "peer overlay frame ref uses a stale or future epoch",
            ));
        }
        if self.revoked.contains(&peer.peer_id) {
            return Err(overlay_policy_error(
                "peer overlay frame ref names a revoked peer",
            ));
        }
        match self.admitted.get(&peer.peer_id) {
            Some(admitted) if admitted == peer => Ok(()),
            Some(_) => Err(overlay_policy_error(
                "peer overlay frame ref does not match admitted member/device binding",
            )),
            None => Err(overlay_policy_error(
                "peer overlay frame ref is not admitted in the current epoch",
            )),
        }
    }
}

/// Peer overlay route refs and loop controls.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRoute {
    /// Current-epoch admitted source peer.
    pub source: PeerOverlayPeerRef,
    /// Ordered content-blind relay peers.
    pub relay_path: Vec<PeerOverlayPeerRef>,
    /// Current-epoch admitted destination peer.
    pub destination: PeerOverlayPeerRef,
    /// Hop/time bound for this frame.
    pub ttl: PeerOverlayTtl,
    /// Opaque loop id for duplicate/loop suppression.
    pub loop_id: PeerOverlayLoopId,
}

impl PeerOverlayRoute {
    /// Validate route refs, TTL, and loop constraints.
    pub fn validate(&self, admitted: &PeerOverlayAdmittedSet) -> Result<(), TransportError> {
        admitted.validate_ref(&self.source)?;
        admitted.validate_ref(&self.destination)?;
        if self.source.peer_id == self.destination.peer_id {
            return Err(overlay_policy_error(
                "peer overlay source and destination must differ",
            ));
        }
        self.ttl.validate()?;
        self.loop_id.validate()?;
        if self.relay_path.is_empty() {
            return Err(overlay_policy_error(
                "peer overlay route requires at least one relay peer",
            ));
        }
        if self.relay_path.len() > usize::from(PEER_OVERLAY_MAX_RELAY_HOPS) {
            return Err(overlay_policy_error(
                "peer overlay relay path exceeds hop cap",
            ));
        }
        let mut seen_relays = BTreeSet::new();
        for relay in &self.relay_path {
            admitted.validate_ref(relay)?;
            if relay.peer_id == self.source.peer_id || relay.peer_id == self.destination.peer_id {
                return Err(overlay_policy_error(
                    "peer overlay relay cannot be source or destination",
                ));
            }
            if !seen_relays.insert(relay.peer_id.clone()) {
                return Err(overlay_policy_error(
                    "peer overlay relay path must not repeat a peer",
                ));
            }
        }
        Ok(())
    }
}

/// Hop/time bound for a peer overlay frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayTtl {
    /// Remaining relay hops. Must be 1..=3 for overlay relay frames.
    pub remaining_hops: u8,
    /// Sender/runtime wall-clock creation timestamp in milliseconds.
    pub created_at_ms: u64,
    /// Sender/runtime wall-clock expiry timestamp in milliseconds.
    pub expires_at_ms: u64,
}

impl PeerOverlayTtl {
    /// Validate hop and wall-clock expiry bounds.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.remaining_hops == 0 || self.remaining_hops > PEER_OVERLAY_MAX_RELAY_HOPS {
            return Err(overlay_policy_error(
                "peer overlay remaining hops must be within the relay hop cap",
            ));
        }
        if self.expires_at_ms <= self.created_at_ms {
            return Err(overlay_policy_error(
                "peer overlay expiry must be after creation time",
            ));
        }
        Ok(())
    }
}

/// OpenMLS/backend auth binding for an opaque overlay frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayAuth {
    /// Commitment to the OpenMLS group id.
    pub group_id_commitment: [u8; 32],
    /// OpenMLS epoch for the protected frame.
    pub epoch: u64,
    /// Source sender leaf index for receiver verification.
    pub sender_leaf_index: u32,
    /// Commitment to the current OpenMLS confirmation tag.
    pub confirmation_tag_commitment: [u8; 32],
    /// Authentication tag from the protected-envelope layer.
    pub frame_auth_tag: Vec<u8>,
}

impl PeerOverlayAuth {
    /// Validate that auth material is present and epoch-bound.
    pub fn validate(&self, admitted: &PeerOverlayAdmittedSet) -> Result<(), TransportError> {
        if self.epoch != admitted.current_epoch {
            return Err(overlay_policy_error(
                "peer overlay auth epoch must match current admitted epoch",
            ));
        }
        if self.group_id_commitment == [0; 32] {
            return Err(overlay_policy_error(
                "peer overlay group id commitment must not be all zero",
            ));
        }
        if self.confirmation_tag_commitment == [0; 32] {
            return Err(overlay_policy_error(
                "peer overlay confirmation commitment must not be all zero",
            ));
        }
        if self.frame_auth_tag.len() < 16 {
            return Err(overlay_policy_error(
                "peer overlay frame auth tag must be at least 16 bytes",
            ));
        }
        Ok(())
    }
}

/// Relay-visible protected payload class.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerOverlayPayloadKind {
    /// Protected group text/control envelope.
    TextControl,
    /// Protected voice/media envelope for a later media task.
    Media,
    /// Protected store-forward envelope for a later storage task.
    StoreForward,
}

/// Opaque protected payload carried by the overlay.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayOpaquePayload {
    /// Payload class used for policy, not decryption.
    pub kind: PeerOverlayPayloadKind,
    /// Sender-produced content key id or media KID.
    pub key_id: Vec<u8>,
    /// Sender monotonic protected-envelope sequence.
    pub sequence: u64,
    /// Commitment to authenticated associated data.
    pub aad_commitment: [u8; 32],
    /// Ciphertext/authenticated bytes. Transport never decrypts this field.
    pub opaque_ciphertext: Vec<u8>,
}

impl PeerOverlayOpaquePayload {
    /// Validate that the payload is complete but opaque.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.key_id.is_empty() {
            return Err(overlay_policy_error(
                "peer overlay key id must not be empty",
            ));
        }
        if self.aad_commitment == [0; 32] {
            return Err(overlay_policy_error(
                "peer overlay AAD commitment must not be all zero",
            ));
        }
        if self.opaque_ciphertext.is_empty() {
            return Err(overlay_policy_error(
                "peer overlay ciphertext must not be empty",
            ));
        }
        Ok(())
    }

    /// Relay-visible bytes for audits; this exposes metadata commitments and ciphertext only.
    #[must_use]
    pub fn relay_visible_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(
            self.key_id.len() + self.aad_commitment.len() + self.opaque_ciphertext.len() + 24,
        );
        bytes.extend_from_slice(match self.kind {
            PeerOverlayPayloadKind::TextControl => b"text_control",
            PeerOverlayPayloadKind::Media => b"media",
            PeerOverlayPayloadKind::StoreForward => b"store_forward",
        });
        bytes.extend_from_slice(&(self.key_id.len() as u64).to_be_bytes());
        bytes.extend_from_slice(&self.key_id);
        bytes.extend_from_slice(&self.sequence.to_be_bytes());
        bytes.extend_from_slice(&self.aad_commitment);
        bytes.extend_from_slice(&self.opaque_ciphertext);
        bytes
    }
}

/// Destination acknowledgement mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerOverlayAckMode {
    /// Destination must authenticate and return a receipt/ack.
    AckRequired,
    /// No destination ack required; not suitable for protected group text delivery.
    BestEffort,
}

/// Redelivery policy bounds for a frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRedelivery {
    /// Maximum delivery attempts for this ack id.
    pub max_attempts: u8,
    /// Maximum relay peers to ask during redelivery.
    pub max_relay_fanout: u8,
    /// Redelivery deadline in sender/runtime milliseconds.
    pub deadline_ms: u64,
}

impl PeerOverlayRedelivery {
    /// Validate retry/fanout bounds against the frame TTL.
    pub fn validate(&self, ttl: &PeerOverlayTtl) -> Result<(), TransportError> {
        if self.max_attempts == 0 {
            return Err(overlay_policy_error(
                "peer overlay redelivery attempts must be non-zero",
            ));
        }
        if self.max_relay_fanout == 0 || self.max_relay_fanout > PEER_OVERLAY_MAX_RELAY_HOPS {
            return Err(overlay_policy_error(
                "peer overlay redelivery fanout must be within the relay hop cap",
            ));
        }
        if self.deadline_ms > ttl.expires_at_ms {
            return Err(overlay_policy_error(
                "peer overlay redelivery deadline must not exceed frame expiry",
            ));
        }
        Ok(())
    }
}

/// Ack/redelivery contract for one overlay frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayDelivery {
    /// Opaque ack id for receiver receipt and redelivery correlation.
    pub ack_id: PeerOverlayAckId,
    /// Whether destination authentication must produce an ack.
    pub ack_mode: PeerOverlayAckMode,
    /// Bounded redelivery behavior.
    pub redelivery: PeerOverlayRedelivery,
}

impl PeerOverlayDelivery {
    /// Validate ack and redelivery bounds.
    pub fn validate(&self, ttl: &PeerOverlayTtl) -> Result<(), TransportError> {
        self.ack_id.validate()?;
        self.redelivery.validate(ttl)
    }
}

/// Carrier class for a future overlay frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerOverlayCarrier {
    /// Direct WebRTC DataChannel between admitted peers.
    DirectWebRtcDataChannel,
    /// Configured TURN-backed WebRTC between admitted peers.
    ConfiguredTurnBackedWebRtc,
    /// Peer-assisted overlay relay between admitted peers.
    PeerAssistedOverlay,
    /// Forbidden provider application relay fallback.
    ProviderApplicationRelay,
}

impl PeerOverlayCarrier {
    /// Validate that a carrier preserves provider-as-signaling-only boundaries.
    pub fn validate(self) -> Result<(), TransportError> {
        if self == Self::ProviderApplicationRelay {
            Err(overlay_policy_error(
                "peer overlay frames must not use providers as application relays",
            ))
        } else {
            Ok(())
        }
    }
}

/// Versioned peer overlay frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayFrame {
    /// Frame schema version.
    pub schema_version: u16,
    /// Carrier boundary for this frame.
    pub carrier: PeerOverlayCarrier,
    /// Current-epoch route refs and loop controls.
    pub route: PeerOverlayRoute,
    /// OpenMLS/backend auth binding.
    pub auth: PeerOverlayAuth,
    /// Ack and redelivery contract.
    pub delivery: PeerOverlayDelivery,
    /// Opaque protected payload.
    pub payload: PeerOverlayOpaquePayload,
}

impl PeerOverlayFrame {
    /// Build a schema-v1 frame from already protected material.
    #[must_use]
    pub fn new(
        carrier: PeerOverlayCarrier,
        route: PeerOverlayRoute,
        auth: PeerOverlayAuth,
        delivery: PeerOverlayDelivery,
        payload: PeerOverlayOpaquePayload,
    ) -> Self {
        Self {
            schema_version: PEER_OVERLAY_FRAME_SCHEMA_VERSION,
            carrier,
            route,
            auth,
            delivery,
            payload,
        }
    }

    /// Validate the protocol frame without decrypting or forwarding it.
    pub fn validate(&self, admitted: &PeerOverlayAdmittedSet) -> Result<(), TransportError> {
        if self.schema_version != PEER_OVERLAY_FRAME_SCHEMA_VERSION {
            return Err(overlay_policy_error(
                "unsupported peer overlay frame schema version",
            ));
        }
        self.carrier.validate()?;
        self.route.validate(admitted)?;
        self.auth.validate(admitted)?;
        if self.auth.epoch != self.route.source.epoch
            || self.auth.epoch != self.route.destination.epoch
        {
            return Err(overlay_policy_error(
                "peer overlay auth epoch must match route refs",
            ));
        }
        self.delivery.validate(&self.route.ttl)?;
        self.payload.validate()
    }
}

fn validate_label(value: &str, label: &str) -> Result<(), TransportError> {
    if value.trim().is_empty() || value.trim() != value || value.len() > 128 {
        Err(overlay_policy_error(format!(
            "{label} must be non-empty trimmed text up to 128 bytes"
        )))
    } else {
        Ok(())
    }
}

fn overlay_policy_error(message: impl Into<String>) -> TransportError {
    TransportError::InvalidConnectivityPolicy(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(index: u8, epoch: u64) -> Result<PeerOverlayPeerRef, TransportError> {
        Ok(PeerOverlayPeerRef::new(
            SignalingPeerId::new(format!("peer-{index}"))?,
            format!("member-{index}"),
            format!("device-{index}"),
            epoch,
        ))
    }

    fn admitted(epoch: u64) -> Result<PeerOverlayAdmittedSet, TransportError> {
        PeerOverlayAdmittedSet::new(
            epoch,
            [peer(1, epoch)?, peer(2, epoch)?, peer(3, epoch)?],
            [],
        )
    }

    fn nonzero_32(byte: u8) -> [u8; 32] {
        [byte; 32]
    }

    fn frame(epoch: u64) -> Result<PeerOverlayFrame, TransportError> {
        let route = PeerOverlayRoute {
            source: peer(1, epoch)?,
            relay_path: vec![peer(2, epoch)?],
            destination: peer(3, epoch)?,
            ttl: PeerOverlayTtl {
                remaining_hops: 2,
                created_at_ms: 1_000,
                expires_at_ms: 2_000,
            },
            loop_id: PeerOverlayLoopId([7; 16]),
        };
        let auth = PeerOverlayAuth {
            group_id_commitment: nonzero_32(1),
            epoch,
            sender_leaf_index: 5,
            confirmation_tag_commitment: nonzero_32(2),
            frame_auth_tag: vec![3; 16],
        };
        let delivery = PeerOverlayDelivery {
            ack_id: PeerOverlayAckId([4; 16]),
            ack_mode: PeerOverlayAckMode::AckRequired,
            redelivery: PeerOverlayRedelivery {
                max_attempts: 3,
                max_relay_fanout: 2,
                deadline_ms: 1_800,
            },
        };
        let payload = PeerOverlayOpaquePayload {
            kind: PeerOverlayPayloadKind::TextControl,
            key_id: b"kid-text".to_vec(),
            sequence: 42,
            aad_commitment: nonzero_32(5),
            opaque_ciphertext: b"DCF1:sealed-ciphertext-only".to_vec(),
        };
        Ok(PeerOverlayFrame::new(
            PeerOverlayCarrier::PeerAssistedOverlay,
            route,
            auth,
            delivery,
            payload,
        ))
    }

    #[test]
    fn validates_current_epoch_admitted_opaque_frame() -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let frame = frame(9)?;

        frame.validate(&admitted)?;
        assert!(!frame
            .payload
            .relay_visible_bytes()
            .windows(b"plaintext message".len())
            .any(|window| window == b"plaintext message"));
        Ok(())
    }

    #[test]
    fn rejects_provider_application_relay_carrier() -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let mut frame = frame(9)?;
        frame.carrier = PeerOverlayCarrier::ProviderApplicationRelay;

        assert!(frame.validate(&admitted).is_err());
        Ok(())
    }

    #[test]
    fn rejects_stale_epoch_and_revoked_peer_refs() -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let stale = frame(8)?;
        assert!(stale.validate(&admitted).is_err());

        let revoked_set = PeerOverlayAdmittedSet::new(
            9,
            [peer(1, 9)?, peer(3, 9)?],
            [SignalingPeerId::new("peer-2")?],
        )?;
        assert!(frame(9)?.validate(&revoked_set).is_err());
        Ok(())
    }

    #[test]
    fn rejects_looping_relay_path_and_bad_ttl() -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let mut looping = frame(9)?;
        looping.route.relay_path = vec![peer(2, 9)?, peer(2, 9)?];
        assert!(looping.validate(&admitted).is_err());

        let mut expired = frame(9)?;
        expired.route.ttl.expires_at_ms = expired.route.ttl.created_at_ms;
        assert!(expired.validate(&admitted).is_err());
        Ok(())
    }

    #[test]
    fn validates_ack_and_redelivery_bounds() -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let mut no_ack = frame(9)?;
        no_ack.delivery.ack_id = PeerOverlayAckId([0; 16]);
        assert!(no_ack.validate(&admitted).is_err());

        let mut deadline_after_ttl = frame(9)?;
        deadline_after_ttl.delivery.redelivery.deadline_ms = 2_001;
        assert!(deadline_after_ttl.validate(&admitted).is_err());
        Ok(())
    }
}
