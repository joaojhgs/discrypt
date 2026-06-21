//! Peer-assisted encrypted overlay protocol contract.
//!
//! This module is intentionally data-only. It validates the shape of future
//! relay frames without selecting routes, authorizing relay candidates,
//! forwarding bytes, or exposing any decrypt/key path.

use crate::{SignalingPeerId, TransportError};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
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

/// Source of relay authority after backend/OpenMLS or governance verification.
///
/// This enum records which already-verified authority path produced a relay
/// authorization set. It does not verify OpenMLS commits or governance
/// signatures itself; callers must do that before constructing the set.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PeerOverlayRelayAuthoritySource {
    /// Authority derived from current persisted OpenMLS/backend group state.
    OpenMlsCurrentEpoch,
    /// Authority derived from a signed governance grant that the caller has verified.
    SignedGovernanceGrant {
        /// Backend/OpenMLS-governed signer member id.
        signer_member_id: String,
        /// Backend/OpenMLS-governed signer device id.
        signer_device_id: String,
    },
}

impl PeerOverlayRelayAuthoritySource {
    fn validate(&self) -> Result<(), TransportError> {
        match self {
            Self::OpenMlsCurrentEpoch => Ok(()),
            Self::SignedGovernanceGrant {
                signer_member_id,
                signer_device_id,
            } => {
                validate_label(
                    signer_member_id,
                    "peer overlay relay authority signer member id",
                )?;
                validate_label(
                    signer_device_id,
                    "peer overlay relay authority signer device id",
                )
            }
        }
    }
}

/// Explicit relay authorization for one admitted current-epoch relay peer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRelayAuthorization {
    /// Authority source that produced this relay grant.
    pub source: PeerOverlayRelayAuthoritySource,
    /// Commitment to the OpenMLS group id.
    pub group_id_commitment: [u8; 32],
    /// Current OpenMLS/backend epoch.
    pub epoch: u64,
    /// Commitment to the current OpenMLS confirmation tag.
    pub confirmation_tag_commitment: [u8; 32],
    /// Authorized admitted relay peer.
    pub relay: PeerOverlayPeerRef,
}

impl PeerOverlayRelayAuthorization {
    fn validate_for(
        &self,
        relay: &PeerOverlayPeerRef,
        auth: &PeerOverlayAuth,
    ) -> Result<(), TransportError> {
        self.source.validate()?;
        validate_nonzero_32(
            self.group_id_commitment,
            "peer overlay relay authority group id commitment",
        )?;
        validate_nonzero_32(
            self.confirmation_tag_commitment,
            "peer overlay relay authority confirmation commitment",
        )?;
        if self.epoch != auth.epoch {
            return Err(overlay_policy_error(
                "peer overlay relay authority epoch must match frame auth epoch",
            ));
        }
        if self.group_id_commitment != auth.group_id_commitment {
            return Err(overlay_policy_error(
                "peer overlay relay authority group commitment must match frame auth",
            ));
        }
        if self.confirmation_tag_commitment != auth.confirmation_tag_commitment {
            return Err(overlay_policy_error(
                "peer overlay relay authority confirmation commitment must match frame auth",
            ));
        }
        if self.relay != *relay {
            return Err(overlay_policy_error(
                "peer overlay relay authority does not match route relay ref",
            ));
        }
        Ok(())
    }
}

/// Current relay authority set supplied by backend/OpenMLS state or signed governance evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRelayAuthoritySet {
    /// Authority source that produced the relay set.
    pub source: PeerOverlayRelayAuthoritySource,
    /// Current group epoch for relay authority.
    pub current_epoch: u64,
    /// Commitment to the OpenMLS group id.
    pub group_id_commitment: [u8; 32],
    /// Commitment to the current OpenMLS confirmation tag.
    pub confirmation_tag_commitment: [u8; 32],
    authorized_relays: BTreeMap<SignalingPeerId, PeerOverlayPeerRef>,
}

impl PeerOverlayRelayAuthoritySet {
    /// Build a relay authority set from already-verified OpenMLS/backend state.
    pub fn from_openmls_current_epoch(
        admitted: &PeerOverlayAdmittedSet,
        group_id_commitment: [u8; 32],
        confirmation_tag_commitment: [u8; 32],
        relays: impl IntoIterator<Item = PeerOverlayPeerRef>,
    ) -> Result<Self, TransportError> {
        Self::new(
            PeerOverlayRelayAuthoritySource::OpenMlsCurrentEpoch,
            admitted,
            group_id_commitment,
            confirmation_tag_commitment,
            relays,
        )
    }

    /// Build a relay authority set from already-verified signed governance evidence.
    pub fn from_signed_governance_grant(
        signer_member_id: impl Into<String>,
        signer_device_id: impl Into<String>,
        admitted: &PeerOverlayAdmittedSet,
        group_id_commitment: [u8; 32],
        confirmation_tag_commitment: [u8; 32],
        relays: impl IntoIterator<Item = PeerOverlayPeerRef>,
    ) -> Result<Self, TransportError> {
        Self::new(
            PeerOverlayRelayAuthoritySource::SignedGovernanceGrant {
                signer_member_id: signer_member_id.into(),
                signer_device_id: signer_device_id.into(),
            },
            admitted,
            group_id_commitment,
            confirmation_tag_commitment,
            relays,
        )
    }

    fn new(
        source: PeerOverlayRelayAuthoritySource,
        admitted: &PeerOverlayAdmittedSet,
        group_id_commitment: [u8; 32],
        confirmation_tag_commitment: [u8; 32],
        relays: impl IntoIterator<Item = PeerOverlayPeerRef>,
    ) -> Result<Self, TransportError> {
        source.validate()?;
        validate_nonzero_32(
            group_id_commitment,
            "peer overlay relay authority group id commitment",
        )?;
        validate_nonzero_32(
            confirmation_tag_commitment,
            "peer overlay relay authority confirmation commitment",
        )?;
        let mut authorized_relays = BTreeMap::new();
        for relay in relays {
            admitted.validate_ref(&relay)?;
            if authorized_relays
                .insert(relay.peer_id.clone(), relay)
                .is_some()
            {
                return Err(overlay_policy_error(
                    "peer overlay relay authority peers must be unique",
                ));
            }
        }
        if authorized_relays.is_empty() {
            return Err(overlay_policy_error(
                "peer overlay relay authority requires at least one relay peer",
            ));
        }
        Ok(Self {
            source,
            current_epoch: admitted.current_epoch,
            group_id_commitment,
            confirmation_tag_commitment,
            authorized_relays,
        })
    }

    /// Return an explicit authorization proof for one relay ref.
    pub fn authorize_relay(
        &self,
        relay: &PeerOverlayPeerRef,
        auth: &PeerOverlayAuth,
    ) -> Result<PeerOverlayRelayAuthorization, TransportError> {
        if self.current_epoch != auth.epoch {
            return Err(overlay_policy_error(
                "peer overlay relay authority set epoch must match frame auth epoch",
            ));
        }
        if self.group_id_commitment != auth.group_id_commitment {
            return Err(overlay_policy_error(
                "peer overlay relay authority set group commitment must match frame auth",
            ));
        }
        if self.confirmation_tag_commitment != auth.confirmation_tag_commitment {
            return Err(overlay_policy_error(
                "peer overlay relay authority set confirmation commitment must match frame auth",
            ));
        }
        let authorized = self.authorized_relays.get(&relay.peer_id).ok_or_else(|| {
            overlay_policy_error(
                "peer overlay relay ref lacks explicit current-epoch relay authority",
            )
        })?;
        if authorized != relay {
            return Err(overlay_policy_error(
                "peer overlay relay authority does not match admitted member/device binding",
            ));
        }
        Ok(PeerOverlayRelayAuthorization {
            source: self.source.clone(),
            group_id_commitment: self.group_id_commitment,
            epoch: self.current_epoch,
            confirmation_tag_commitment: self.confirmation_tag_commitment,
            relay: authorized.clone(),
        })
    }

    /// Validate every relay hop in a route against explicit relay authority.
    pub fn validate_route_authority(
        &self,
        route: &PeerOverlayRoute,
        auth: &PeerOverlayAuth,
    ) -> Result<Vec<PeerOverlayRelayAuthorization>, TransportError> {
        route
            .relay_path
            .iter()
            .map(|relay| {
                let authorization = self.authorize_relay(relay, auth)?;
                authorization.validate_for(relay, auth)?;
                Ok(authorization)
            })
            .collect()
    }
}

/// Runtime/backend evidence used to rank one relay candidate.
///
/// These diagnostics are intentionally content-blind. They describe transport
/// health and relay contribution only; they do not include message, media, key,
/// provider payload, or group-content data.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRelayCandidateDiagnostics {
    /// Recent round-trip latency estimate in milliseconds.
    pub latency_ms: u32,
    /// Successful health probes in the current ranking window.
    pub successful_health_probes: u32,
    /// Failed health probes in the current ranking window.
    pub failed_health_probes: u32,
    /// Advertised relay egress capacity.
    pub egress_capacity_bytes_per_second: u64,
    /// Current relay load estimate.
    pub current_load_bytes_per_second: u64,
    /// Local estimate of battery/CPU/network cost in basis points.
    pub energy_cost_bps: u16,
    /// Anti-freeload penalty in basis points.
    pub freeload_penalty_bps: u16,
}

impl PeerOverlayRelayCandidateDiagnostics {
    fn validate(&self, policy: &PeerOverlayRelayCandidatePolicy) -> Result<(), TransportError> {
        if self.successful_health_probes == 0 {
            return Err(overlay_policy_error(
                "peer overlay relay candidate requires a successful health probe",
            ));
        }
        if self
            .successful_health_probes
            .saturating_add(self.failed_health_probes)
            == 0
        {
            return Err(overlay_policy_error(
                "peer overlay relay candidate requires health probe evidence",
            ));
        }
        if self.stability_bps() < policy.min_stability_bps {
            return Err(overlay_policy_error(
                "peer overlay relay candidate is below stability policy",
            ));
        }
        if self.energy_cost_bps > policy.max_energy_cost_bps {
            return Err(overlay_policy_error(
                "peer overlay relay candidate exceeds energy policy",
            ));
        }
        if self.egress_capacity_bytes_per_second < policy.min_egress_capacity_bytes_per_second {
            return Err(overlay_policy_error(
                "peer overlay relay candidate lacks required egress capacity",
            ));
        }
        if self.current_load_bytes_per_second >= self.egress_capacity_bytes_per_second {
            return Err(overlay_policy_error(
                "peer overlay relay candidate has no spare relay capacity",
            ));
        }
        Ok(())
    }

    fn stability_bps(&self) -> u32 {
        let total = self
            .successful_health_probes
            .saturating_add(self.failed_health_probes)
            .max(1);
        self.successful_health_probes.saturating_mul(10_000) / total
    }

    fn spare_capacity_bytes_per_second(&self) -> u64 {
        self.egress_capacity_bytes_per_second
            .saturating_sub(self.current_load_bytes_per_second)
    }
}

/// One relay candidate with current admitted-peer and diagnostic evidence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRelayCandidate {
    /// Candidate relay peer.
    pub relay: PeerOverlayPeerRef,
    /// Content-blind ranking diagnostics.
    pub diagnostics: PeerOverlayRelayCandidateDiagnostics,
}

/// Policy gate applied before relay candidate ranking.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRelayCandidatePolicy {
    /// Carrier being ranked. Must be peer-assisted overlay, never provider relay.
    pub carrier: PeerOverlayCarrier,
    /// Minimum relay egress capacity required by the caller.
    pub min_egress_capacity_bytes_per_second: u64,
    /// Minimum health stability in basis points.
    pub min_stability_bps: u32,
    /// Maximum allowed energy cost in basis points.
    pub max_energy_cost_bps: u16,
}

impl Default for PeerOverlayRelayCandidatePolicy {
    fn default() -> Self {
        Self {
            carrier: PeerOverlayCarrier::PeerAssistedOverlay,
            min_egress_capacity_bytes_per_second: 1_024,
            min_stability_bps: 1,
            max_energy_cost_bps: 10_000,
        }
    }
}

impl PeerOverlayRelayCandidatePolicy {
    fn validate(&self) -> Result<(), TransportError> {
        self.carrier.validate()?;
        if self.carrier != PeerOverlayCarrier::PeerAssistedOverlay {
            return Err(overlay_policy_error(
                "peer overlay relay candidate ranking requires peer-assisted overlay carrier",
            ));
        }
        if self.min_egress_capacity_bytes_per_second == 0 {
            return Err(overlay_policy_error(
                "peer overlay relay candidate policy requires non-zero capacity",
            ));
        }
        if self.min_stability_bps > 10_000 {
            return Err(overlay_policy_error(
                "peer overlay relay candidate policy stability must be <= 10000 bps",
            ));
        }
        if self.max_energy_cost_bps > 10_000 {
            return Err(overlay_policy_error(
                "peer overlay relay candidate policy energy cost must be <= 10000 bps",
            ));
        }
        Ok(())
    }
}

/// Ranked relay candidate plus the explicit authority proof used for ranking.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PeerOverlayRankedRelayCandidate {
    /// Candidate relay peer.
    pub relay: PeerOverlayPeerRef,
    /// Explicit current-epoch relay authorization.
    pub authorization: PeerOverlayRelayAuthorization,
    /// Deterministic score. Higher is better.
    pub score: i64,
    /// Content-blind diagnostics used to produce the score.
    pub diagnostics: PeerOverlayRelayCandidateDiagnostics,
}

/// Rank relay candidates after admission, authorization, health, and policy gates.
pub fn rank_relay_candidates(
    admitted: &PeerOverlayAdmittedSet,
    relay_authority: &PeerOverlayRelayAuthoritySet,
    auth: &PeerOverlayAuth,
    policy: &PeerOverlayRelayCandidatePolicy,
    candidates: impl IntoIterator<Item = PeerOverlayRelayCandidate>,
) -> Result<Vec<PeerOverlayRankedRelayCandidate>, TransportError> {
    policy.validate()?;
    auth.validate(admitted)?;
    if relay_authority.current_epoch != admitted.current_epoch {
        return Err(overlay_policy_error(
            "peer overlay relay candidate authority epoch must match admitted epoch",
        ));
    }

    let mut seen = BTreeSet::new();
    let mut ranked = Vec::new();
    for candidate in candidates {
        admitted.validate_ref(&candidate.relay)?;
        if !seen.insert(candidate.relay.peer_id.clone()) {
            return Err(overlay_policy_error(
                "peer overlay relay candidates must be unique",
            ));
        }
        candidate.diagnostics.validate(policy)?;
        let authorization = relay_authority.authorize_relay(&candidate.relay, auth)?;
        let score = score_relay_candidate(&candidate.diagnostics);
        ranked.push(PeerOverlayRankedRelayCandidate {
            relay: candidate.relay,
            authorization,
            score,
            diagnostics: candidate.diagnostics,
        });
    }
    if ranked.is_empty() {
        return Err(overlay_policy_error(
            "peer overlay relay candidate ranking requires at least one candidate",
        ));
    }
    ranked.sort_by(compare_ranked_relay_candidates);
    Ok(ranked)
}

fn score_relay_candidate(diagnostics: &PeerOverlayRelayCandidateDiagnostics) -> i64 {
    let latency_penalty = i64::from(diagnostics.latency_ms.max(1)) * 100;
    let stability_bonus = i64::from(diagnostics.stability_bps()) * 100;
    let capped_capacity = diagnostics
        .spare_capacity_bytes_per_second()
        .min(10_000_000);
    let capacity_bonus = i64::try_from(capped_capacity / 1_024).unwrap_or(i64::MAX);
    let energy_penalty = i64::from(diagnostics.energy_cost_bps) * 10;
    let freeload_penalty = i64::from(diagnostics.freeload_penalty_bps) * 10;

    stability_bonus + capacity_bonus - latency_penalty - energy_penalty - freeload_penalty
}

fn compare_ranked_relay_candidates(
    a: &PeerOverlayRankedRelayCandidate,
    b: &PeerOverlayRankedRelayCandidate,
) -> Ordering {
    b.score
        .cmp(&a.score)
        .then_with(|| a.relay.peer_id.cmp(&b.relay.peer_id))
        .then_with(|| a.relay.member_id.cmp(&b.relay.member_id))
        .then_with(|| a.relay.device_id.cmp(&b.relay.device_id))
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
        validate_nonzero_32(self.group_id_commitment, "peer overlay group id commitment")?;
        validate_nonzero_32(
            self.confirmation_tag_commitment,
            "peer overlay confirmation commitment",
        )?;
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

    /// Validate a frame and require explicit relay authorization for every relay hop.
    pub fn validate_relay_authorized(
        &self,
        admitted: &PeerOverlayAdmittedSet,
        relay_authority: &PeerOverlayRelayAuthoritySet,
    ) -> Result<Vec<PeerOverlayRelayAuthorization>, TransportError> {
        self.validate(admitted)?;
        relay_authority.validate_route_authority(&self.route, &self.auth)
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

fn validate_nonzero_32(value: [u8; 32], label: &str) -> Result<(), TransportError> {
    if value == [0; 32] {
        Err(overlay_policy_error(format!(
            "{label} must not be all zero"
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

    fn relay_authority(
        admitted: &PeerOverlayAdmittedSet,
        epoch: u64,
    ) -> Result<PeerOverlayRelayAuthoritySet, TransportError> {
        PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            admitted,
            nonzero_32(1),
            nonzero_32(2),
            [peer(2, epoch)?],
        )
    }

    fn relay_authority_for(
        admitted: &PeerOverlayAdmittedSet,
        relays: impl IntoIterator<Item = PeerOverlayPeerRef>,
    ) -> Result<PeerOverlayRelayAuthoritySet, TransportError> {
        PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            admitted,
            nonzero_32(1),
            nonzero_32(2),
            relays,
        )
    }

    fn auth(epoch: u64) -> PeerOverlayAuth {
        PeerOverlayAuth {
            group_id_commitment: nonzero_32(1),
            epoch,
            sender_leaf_index: 5,
            confirmation_tag_commitment: nonzero_32(2),
            frame_auth_tag: vec![3; 16],
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn candidate(
        index: u8,
        epoch: u64,
        latency_ms: u32,
        successful_health_probes: u32,
        failed_health_probes: u32,
        capacity: u64,
        load: u64,
        energy_cost_bps: u16,
        freeload_penalty_bps: u16,
    ) -> Result<PeerOverlayRelayCandidate, TransportError> {
        Ok(PeerOverlayRelayCandidate {
            relay: peer(index, epoch)?,
            diagnostics: PeerOverlayRelayCandidateDiagnostics {
                latency_ms,
                successful_health_probes,
                failed_health_probes,
                egress_capacity_bytes_per_second: capacity,
                current_load_bytes_per_second: load,
                energy_cost_bps,
                freeload_penalty_bps,
            },
        })
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
    fn relay_authorization_accepts_only_explicit_current_epoch_relays() -> Result<(), TransportError>
    {
        let admitted = admitted(9)?;
        let frame = frame(9)?;
        let authority = relay_authority(&admitted, 9)?;

        let authorizations = frame.validate_relay_authorized(&admitted, &authority)?;
        assert_eq!(authorizations.len(), 1);
        assert_eq!(authorizations[0].relay, peer(2, 9)?);
        assert_eq!(authorizations[0].epoch, 9);
        Ok(())
    }

    #[test]
    fn ranks_authorized_current_epoch_candidates_by_health_capacity_and_penalties(
    ) -> Result<(), TransportError> {
        let epoch = 9;
        let admitted = PeerOverlayAdmittedSet::new(
            epoch,
            [
                peer(1, epoch)?,
                peer(2, epoch)?,
                peer(3, epoch)?,
                peer(4, epoch)?,
            ],
            [],
        )?;
        let authority = relay_authority_for(&admitted, [peer(2, epoch)?, peer(4, epoch)?])?;

        let ranked = rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [
                candidate(2, epoch, 12, 10, 0, 96_000, 8_000, 100, 8_000)?,
                candidate(4, epoch, 20, 10, 0, 192_000, 8_000, 100, 0)?,
            ],
        )?;

        assert_eq!(ranked[0].relay.peer_id.0, "peer-4");
        assert!(ranked[0].score > ranked[1].score);
        assert_eq!(ranked[0].authorization.relay, peer(4, epoch)?);
        Ok(())
    }

    #[test]
    fn relay_candidate_ties_are_deterministic_by_peer_identity() -> Result<(), TransportError> {
        let epoch = 9;
        let admitted = PeerOverlayAdmittedSet::new(
            epoch,
            [
                peer(1, epoch)?,
                peer(2, epoch)?,
                peer(3, epoch)?,
                peer(4, epoch)?,
            ],
            [],
        )?;
        let authority = relay_authority_for(&admitted, [peer(4, epoch)?, peer(2, epoch)?])?;

        let ranked = rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [
                candidate(4, epoch, 25, 8, 0, 64_000, 4_000, 100, 0)?,
                candidate(2, epoch, 25, 8, 0, 64_000, 4_000, 100, 0)?,
            ],
        )?;

        assert_eq!(ranked[0].relay.peer_id.0, "peer-2");
        assert_eq!(ranked[1].relay.peer_id.0, "peer-4");
        Ok(())
    }

    #[test]
    fn relay_candidate_ranking_rejects_revoked_stale_and_non_member_candidates(
    ) -> Result<(), TransportError> {
        let epoch = 9;
        let active = admitted(epoch)?;
        let active_authority = relay_authority(&active, epoch)?;
        let revoked = PeerOverlayAdmittedSet::new(
            epoch,
            [peer(1, epoch)?, peer(3, epoch)?],
            [SignalingPeerId::new("peer-2")?],
        )?;

        assert!(rank_relay_candidates(
            &revoked,
            &active_authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [candidate(2, epoch, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());

        assert!(rank_relay_candidates(
            &active,
            &active_authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [candidate(2, epoch - 1, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());

        assert!(rank_relay_candidates(
            &active,
            &active_authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [candidate(4, epoch, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn relay_candidate_ranking_rejects_missing_authority_unhealthy_and_policy_disallowed(
    ) -> Result<(), TransportError> {
        let epoch = 9;
        let admitted = PeerOverlayAdmittedSet::new(
            epoch,
            [
                peer(1, epoch)?,
                peer(2, epoch)?,
                peer(3, epoch)?,
                peer(4, epoch)?,
            ],
            [],
        )?;
        let authority = relay_authority(&admitted, epoch)?;

        assert!(rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [candidate(4, epoch, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());

        assert!(rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy::default(),
            [candidate(2, epoch, 20, 0, 5, 64_000, 0, 0, 0)?],
        )
        .is_err());

        assert!(rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy {
                min_egress_capacity_bytes_per_second: 128_000,
                ..PeerOverlayRelayCandidatePolicy::default()
            },
            [candidate(2, epoch, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());

        assert!(rank_relay_candidates(
            &admitted,
            &authority,
            &auth(epoch),
            &PeerOverlayRelayCandidatePolicy {
                carrier: PeerOverlayCarrier::ProviderApplicationRelay,
                ..PeerOverlayRelayCandidatePolicy::default()
            },
            [candidate(2, epoch, 20, 5, 0, 64_000, 0, 0, 0)?],
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn signed_governance_relay_authority_binds_group_epoch_and_confirmation(
    ) -> Result<(), TransportError> {
        let admitted = admitted(9)?;
        let frame = frame(9)?;
        let authority = PeerOverlayRelayAuthoritySet::from_signed_governance_grant(
            "owner-member",
            "owner-device",
            &admitted,
            nonzero_32(1),
            nonzero_32(2),
            [peer(2, 9)?],
        )?;

        assert!(frame
            .validate_relay_authorized(&admitted, &authority)
            .is_ok());

        let wrong_group = PeerOverlayRelayAuthoritySet::from_signed_governance_grant(
            "owner-member",
            "owner-device",
            &admitted,
            nonzero_32(8),
            nonzero_32(2),
            [peer(2, 9)?],
        )?;
        assert!(frame
            .validate_relay_authorized(&admitted, &wrong_group)
            .is_err());
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
    fn revoked_non_member_and_stale_relays_lack_relay_authority() -> Result<(), TransportError> {
        let active = admitted(9)?;
        let revoked = PeerOverlayAdmittedSet::new(
            9,
            [peer(1, 9)?, peer(3, 9)?],
            [SignalingPeerId::new("peer-2")?],
        )?;
        assert!(PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &revoked,
            nonzero_32(1),
            nonzero_32(2),
            [peer(2, 9)?],
        )
        .is_err());

        assert!(PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &active,
            nonzero_32(1),
            nonzero_32(2),
            [peer(4, 9)?],
        )
        .is_err());

        assert!(PeerOverlayRelayAuthoritySet::from_openmls_current_epoch(
            &active,
            nonzero_32(1),
            nonzero_32(2),
            [peer(2, 8)?],
        )
        .is_err());
        Ok(())
    }

    #[test]
    fn route_graph_membership_alone_is_not_relay_authority() -> Result<(), TransportError> {
        let admitted = PeerOverlayAdmittedSet::new(
            9,
            [peer(1, 9)?, peer(2, 9)?, peer(3, 9)?, peer(4, 9)?],
            [],
        )?;
        let mut frame = frame(9)?;
        frame.route.relay_path = vec![peer(4, 9)?];
        let authority = relay_authority(&admitted, 9)?;

        assert!(frame
            .validate_relay_authorized(&admitted, &authority)
            .is_err());
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
