use chrono::{Duration, Utc};
use discrypt_admission::{
    InviteEndpointPolicy, InviteError, InviteSignalingMetadata, InviteStore, InviteTrustMetadata,
    signaling_fingerprint_for_endpoint,
};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

fn trust_for(endpoint: &str) -> InviteTrustMetadata {
    InviteTrustMetadata::new(
        signaling_fingerprint_for_endpoint(endpoint),
        "signed endpoint fingerprint; verify before MLS Welcome",
    )
    .expect("test metadata has valid trust")
}

#[test]
fn signed_invite_descriptor_covers_signaling_metadata() {
    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint),
    )
    .expect("metadata is valid");
    let mut store = InviteStore::new();
    let invite = store
        .issue_invite_with_metadata(
            b"room secret never serialized",
            now + Duration::minutes(5),
            3,
            metadata,
            &issuer,
        )
        .expect("valid metadata issues invite");

    assert!(invite.verify_issuer_signature().is_ok());

    let mut tampered_endpoint = invite.clone();
    tampered_endpoint.signaling_metadata.signaling_endpoint =
        "https://other.example.invalid/v1/rendezvous".to_owned();
    assert_eq!(
        tampered_endpoint.verify_issuer_signature(),
        Err(InviteError::InvalidIssuerSignature)
    );

    let mut tampered_trust = invite.clone();
    tampered_trust
        .signaling_metadata
        .trust
        .trust_status
        .push_str(" (changed)");
    assert_eq!(
        tampered_trust.verify_issuer_signature(),
        Err(InviteError::InvalidIssuerSignature)
    );
}

#[test]
fn invite_metadata_rejects_invalid_endpoint_and_trust() {
    let endpoint = "https://signal.example.invalid/v1/rendezvous";

    assert_eq!(
        InviteSignalingMetadata::new(
            "http://relay.example.invalid/v1/rendezvous",
            InviteEndpointPolicy::ProductionTls,
            trust_for(endpoint),
        ),
        Err(InviteError::InvalidSignalingEndpoint)
    );

    assert_eq!(
        InviteSignalingMetadata::new(
            " https://signal.example.invalid/v1/rendezvous",
            InviteEndpointPolicy::ProductionTls,
            trust_for(endpoint),
        ),
        Err(InviteError::InvalidSignalingEndpoint)
    );

    assert_eq!(
        InviteTrustMetadata::new("not-a-hex-fingerprint", "fingerprint pinned"),
        Err(InviteError::InvalidTrustMetadata)
    );

    assert_eq!(
        InviteTrustMetadata::new(signaling_fingerprint_for_endpoint(endpoint), "   "),
        Err(InviteError::InvalidTrustMetadata)
    );
}

#[test]
fn serialized_invite_descriptor_redacts_room_secret_but_keeps_join_metadata() {
    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let room_secret = b"room-secret:private-lab:super-secret-token";
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint),
    )
    .expect("metadata is valid");
    let mut store = InviteStore::new();
    let invite = store
        .issue_invite_with_metadata(room_secret, now + Duration::minutes(5), 1, metadata, &issuer)
        .expect("valid metadata issues invite");

    let serialized = serde_json::to_string(&invite).expect("descriptor serializes");

    assert!(serialized.contains(endpoint));
    assert!(serialized.contains("signaling_fingerprint"));
    assert!(serialized.contains("production_tls"));
    assert!(!serialized.contains("super-secret-token"));
    assert!(!serialized.contains("room-secret:private-lab"));
    assert!(!serialized.contains("room_secret="));
}
