use chrono::{Duration, Utc};
use discrypt_admission::{
    signaling_fingerprint_for_endpoint, InviteEndpointPolicy, InviteError, InviteSignalingMetadata,
    InviteStore, InviteTrustMetadata,
};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;

fn trust_for(endpoint: &str) -> Result<InviteTrustMetadata, InviteError> {
    InviteTrustMetadata::new(
        signaling_fingerprint_for_endpoint(endpoint),
        "signed endpoint fingerprint; verify before MLS Welcome",
    )
}

#[test]
fn signed_invite_descriptor_covers_signaling_metadata() -> Result<(), Box<dyn std::error::Error>> {
    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint)?,
    )?;
    let mut store = InviteStore::new();
    let invite = store.issue_invite_with_metadata(
        b"room secret never serialized",
        now + Duration::minutes(5),
        3,
        metadata,
        &issuer,
    )?;

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
    Ok(())
}

#[test]
fn invite_metadata_rejects_invalid_endpoint_and_trust() -> Result<(), Box<dyn std::error::Error>> {
    let endpoint = "https://signal.example.invalid/v1/rendezvous";

    assert_eq!(
        InviteSignalingMetadata::new(
            "http://relay.example.invalid/v1/rendezvous",
            InviteEndpointPolicy::ProductionTls,
            trust_for(endpoint)?,
        ),
        Err(InviteError::InvalidSignalingEndpoint)
    );

    assert_eq!(
        InviteSignalingMetadata::new(
            " https://signal.example.invalid/v1/rendezvous",
            InviteEndpointPolicy::ProductionTls,
            trust_for(endpoint)?,
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
    Ok(())
}

#[test]
fn serialized_invite_descriptor_redacts_room_secret_but_keeps_join_metadata(
) -> Result<(), Box<dyn std::error::Error>> {
    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let room_secret = b"room-secret:private-lab:super-secret-token";
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint)?,
    )?;
    let mut store = InviteStore::new();
    let invite = store.issue_invite_with_metadata(
        room_secret,
        now + Duration::minutes(5),
        1,
        metadata,
        &issuer,
    )?;

    let serialized = serde_json::to_string(&invite)?;

    assert!(serialized.contains(endpoint));
    assert!(serialized.contains("signaling_fingerprint"));
    assert!(serialized.contains("production_tls"));
    assert!(!serialized.contains("super-secret-token"));
    assert!(!serialized.contains("room-secret:private-lab"));
    assert!(!serialized.contains("room_secret="));
    Ok(())
}

#[test]
fn signed_invite_metadata_resolves_typed_ice_config_with_group_precedence(
) -> Result<(), Box<dyn std::error::Error>> {
    use discrypt_transport::{
        ConnectivityPlanner, Endpoint, FallbackLeg, IceEndpointPolicy, SimulatedNat,
        TurnServerConfig,
    };

    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let invite_ice = IceEndpointPolicy::new(
        vec![Endpoint::new("stun:invite.example.invalid:3478")],
        vec![TurnServerConfig::new(
            Endpoint::new("turns:invite.example.invalid:5349"),
            Some("invite-user".to_owned()),
            Some("invite-turn-secret".to_owned()),
            Some("2026-05-29T17:00:00Z".to_owned()),
        )],
    )?;
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint)?,
    )?
    .with_ice_endpoint_policy(invite_ice)?;
    let mut store = InviteStore::new();
    let invite = store.issue_invite_with_metadata(
        b"room secret never serialized",
        now + Duration::minutes(5),
        3,
        metadata,
        &issuer,
    )?;

    let group_ice = IceEndpointPolicy::new(
        vec![Endpoint::new("stuns:group.example.invalid:5349")],
        vec![TurnServerConfig::new(
            Endpoint::new("turn:group.example.invalid:3478"),
            None,
            None,
            None,
        )],
    )?;
    let ice_config = invite.ice_server_config(Some(&group_ice))?;
    let connectivity = ice_config.to_connectivity_config();
    let direct = ConnectivityPlanner::plan(&connectivity, SimulatedNat::direct())?;
    let turn = ConnectivityPlanner::plan(&connectivity, SimulatedNat::turn_only())?;

    assert_eq!(
        direct.endpoint,
        Endpoint::new("stuns:group.example.invalid:5349")
    );
    assert_eq!(turn.selected, FallbackLeg::Turn);
    assert_eq!(
        turn.endpoint,
        Endpoint::new("turn:group.example.invalid:3478")
    );
    assert!(!format!("{ice_config:?}").contains("invite-turn-secret"));
    Ok(())
}

#[test]
fn invite_metadata_rejects_invalid_signed_ice_endpoint_policy(
) -> Result<(), Box<dyn std::error::Error>> {
    use discrypt_transport::{Endpoint, IceEndpointPolicy};

    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let invalid_ice = IceEndpointPolicy {
        stun_servers: vec![Endpoint::new("https://not-stun.example.invalid")],
        turn_servers: vec![],
    };

    assert_eq!(
        InviteSignalingMetadata::new(
            endpoint,
            InviteEndpointPolicy::ProductionTls,
            trust_for(endpoint)?,
        )?
        .with_ice_endpoint_policy(invalid_ice),
        Err(InviteError::InvalidEndpointPolicy)
    );
    Ok(())
}
