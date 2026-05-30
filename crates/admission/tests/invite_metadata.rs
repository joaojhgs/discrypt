use chrono::{Duration, Utc};
use discrypt_admission::{
    signaling_fingerprint_for_endpoint, DmInviteBootstrap, GroupInviteBootstrap,
    InviteBootstrapMetadata, InviteEndpointPolicy, InviteError, InviteKind,
    InviteSignalingAdapterKind, InviteSignalingMetadata, InviteSignalingProfile, InviteStore,
    InviteTrustMetadata, INVITE_CONNECTIVITY_SCHEMA_VERSION,
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

#[test]
fn signed_invite_ice_config_rejects_expired_turn_credentials_at_parse_time(
) -> Result<(), Box<dyn std::error::Error>> {
    use discrypt_transport::{Endpoint, IceEndpointPolicy, TurnServerConfig};

    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let expired_ice = IceEndpointPolicy::new(
        vec![Endpoint::new("stun:invite.example.invalid:3478")],
        vec![TurnServerConfig::new(
            Endpoint::new("turns:invite.example.invalid:5349"),
            Some("joiner".to_owned()),
            Some("expired-secret".to_owned()),
            Some((now - Duration::minutes(1)).to_rfc3339()),
        )],
    )?;
    let metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint)?,
    )?
    .with_ice_endpoint_policy(expired_ice)?;
    let mut store = InviteStore::new();
    let invite = store.issue_invite_with_metadata(
        b"room secret never serialized",
        now + Duration::minutes(5),
        1,
        metadata,
        &issuer,
    )?;

    assert_eq!(
        invite.ice_server_config_at(None, now),
        Err(InviteError::InvalidEndpointPolicy)
    );
    Ok(())
}

fn test_commitment(seed: char) -> String {
    seed.to_string().repeat(64)
}

fn test_bootstrap_profile(scope: &str) -> InviteSignalingProfile {
    InviteSignalingProfile {
        profile_id: "mqtt-default".to_owned(),
        adapter_kind: InviteSignalingAdapterKind::Mqtt,
        endpoints: vec!["wss://mqtt.example.invalid/mqtt".to_owned()],
        room_topic_commitment: scope.to_owned(),
        trust_fingerprint: test_commitment('d'),
        ttl_seconds: 300,
        metadata_posture: "hashed_topic".to_owned(),
        rate_limit_policy: "bounded publish/take with provider backoff".to_owned(),
        capabilities: vec!["presence_ttl".to_owned(), "trickle_ice".to_owned()],
        provider_policy_version: discrypt_admission::INVITE_PROVIDER_POLICY_VERSION,
        endpoint_allowlist_commitments: vec![test_commitment('e')],
        provider_rotation_policy: "rotate by issuing a fresh signed invite/connectivity policy"
            .to_owned(),
    }
}

#[test]
fn signed_invite_descriptor_covers_group_and_dm_bootstrap_metadata(
) -> Result<(), Box<dyn std::error::Error>> {
    let issuer = SigningKey::generate(&mut OsRng);
    let now = Utc::now();
    let endpoint = "https://signal.example.invalid/v1/rendezvous";
    let signaling_metadata = InviteSignalingMetadata::new(
        endpoint,
        InviteEndpointPolicy::ProductionTls,
        trust_for(endpoint)?,
    )?;

    let group_scope = test_commitment('a');
    let group_bootstrap = InviteBootstrapMetadata::group_join(
        group_scope.clone(),
        vec![test_bootstrap_profile(&group_scope)],
        GroupInviteBootstrap {
            group_identity_commitment: group_scope.clone(),
            role_admission_policy_commitment: test_commitment('b'),
            channel_policy_commitment: test_commitment('c'),
        },
    )?;
    let mut store = InviteStore::new();
    let group_invite = store.issue_invite_with_bootstrap_metadata(
        b"room-secret:Private Lab:super-secret-token",
        now + Duration::minutes(5),
        2,
        signaling_metadata.clone(),
        group_bootstrap,
        &issuer,
    )?;

    assert!(group_invite.verify_issuer_signature().is_ok());
    let Some(metadata) = group_invite.bootstrap_metadata.as_ref() else {
        return Err("bootstrap metadata missing".into());
    };
    assert_eq!(
        metadata.connectivity_schema_version,
        INVITE_CONNECTIVITY_SCHEMA_VERSION
    );
    assert_eq!(metadata.invite_kind, InviteKind::GroupJoin);
    assert!(metadata.group_bootstrap.is_some());
    assert!(metadata.dm_bootstrap.is_none());

    let serialized = serde_json::to_string(&group_invite)?;
    assert!(serialized.contains("group_join"));
    assert!(serialized.contains("mqtt"));
    assert!(!serialized.contains("Private Lab"));
    assert!(!serialized.contains("super-secret-token"));

    let mut tampered = group_invite.clone();
    let Some(tampered_metadata) = tampered.bootstrap_metadata.as_mut() else {
        return Err("bootstrap metadata missing".into());
    };
    tampered_metadata.signaling_profiles[0].endpoints[0].push_str("/tampered");
    assert_eq!(
        tampered.verify_issuer_signature(),
        Err(InviteError::InvalidIssuerSignature)
    );

    let dm_scope = test_commitment('e');
    let dm_bootstrap = InviteBootstrapMetadata::dm_contact(
        dm_scope.clone(),
        vec![test_bootstrap_profile(&dm_scope)],
        DmInviteBootstrap {
            inviter_identity_commitment: test_commitment('f'),
            contact_token_commitment: test_commitment('1'),
            reply_rendezvous_commitment: test_commitment('2'),
        },
    )?;
    let dm_invite = store.issue_invite_with_bootstrap_metadata(
        b"dm-contact-secret:alias:hidden-token",
        now + Duration::minutes(5),
        1,
        signaling_metadata,
        dm_bootstrap,
        &issuer,
    )?;

    assert!(dm_invite.verify_issuer_signature().is_ok());
    let Some(dm_metadata) = dm_invite.bootstrap_metadata.as_ref() else {
        return Err("dm bootstrap metadata missing".into());
    };
    assert_eq!(dm_metadata.invite_kind, InviteKind::DmContact);
    assert!(dm_metadata.dm_bootstrap.is_some());
    assert!(dm_metadata.group_bootstrap.is_none());
    let serialized_dm = serde_json::to_string(&dm_invite)?;
    assert!(serialized_dm.contains("dm_contact"));
    assert!(!serialized_dm.contains("hidden-token"));
    Ok(())
}
