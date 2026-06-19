#![cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
use discrypt_transport::probe_provider_webrtc_datachannel_request_response_roundtrip;
use discrypt_transport::{
    derive_scope_commitment, probe_provider_webrtc_datachannel_roundtrip,
    start_provider_webrtc_text_control_answer_runtime_with_answerer,
    start_provider_webrtc_text_control_offer_runtime, AdapterTrustLabel, ConnectivityScopeLevel,
    ConversationScope, Endpoint, IceServerConfig, ProviderTextControlRuntimePeerRole,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingPeerId, SignalingProviderEndpoint, TransportError,
};
#[cfg(feature = "mqtt-adapter")]
use discrypt_transport::{
    probe_provider_webrtc_datachannel_request_response_with_config, TurnServerConfig,
    WebRtcIceTransportPolicy, WebRtcNegotiationConfig,
};
use rand::RngCore;
#[cfg(feature = "mqtt-adapter")]
use serde_json::json;
#[cfg(feature = "mqtt-adapter")]
use sha2::{Digest, Sha256};
#[cfg(feature = "mqtt-adapter")]
use std::path::PathBuf;

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(feature = "mqtt-adapter")]
fn public_mqtt_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-mqtt-webrtc-datachannel-e2e".to_owned(),
        kind: SignalingAdapterKind::Mqtt,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: discrypt_transport::ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public mqtt",
            "public broker; opaque WebRTC negotiation envelopes only",
        )?,
    })
}

#[cfg(feature = "nostr-adapter")]
fn public_nostr_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-nostr-webrtc-datachannel-e2e".to_owned(),
        kind: SignalingAdapterKind::Nostr,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: discrypt_transport::ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public nostr",
            "public relay; opaque WebRTC negotiation envelopes only",
        )?,
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn public_ipfs_profile(endpoints: Vec<String>) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-ipfs-webrtc-datachannel-e2e".to_owned(),
        kind: SignalingAdapterKind::IpfsPubsub,
        endpoints: endpoints
            .into_iter()
            .map(|endpoint| {
                SignalingProviderEndpoint::new(
                    Endpoint::new(endpoint),
                    SignalingEndpointSecurity::ProductionTls,
                )
            })
            .collect(),
        metadata_posture: discrypt_transport::ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public ipfs_pubsub",
            "explicit libp2p direct topic peers; opaque WebRTC negotiation envelopes only",
        )?,
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn public_ipfs_direct_topic_peer_endpoints() -> Result<Vec<String>, TransportError> {
    let endpoints = std::env::var("DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS")
        .map_err(|_| {
            TransportError::SignalingAdapter(
                "DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS is required for public IPFS WebRTC E2E"
                    .to_owned(),
            )
        })?
        .split(',')
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if endpoints.is_empty() || endpoints.iter().any(|endpoint| !endpoint.contains("/p2p/")) {
        return Err(TransportError::InvalidConnectivityPolicy(
            "public IPFS WebRTC E2E requires explicit direct topic-peer multiaddrs with /p2p/<peer-id>"
                .to_owned(),
        ));
    }
    Ok(endpoints)
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn discrypt_rendezvous_trust_fingerprint_for_endpoint(endpoint: &str) -> String {
    use sha2::Digest as _;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"external-signaling-endpoint-fingerprint-v1");
    hasher.update(endpoint.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
fn public_discrypt_rendezvous_profile(
    endpoint: String,
) -> Result<SignalingAdapterProfile, TransportError> {
    let mut provider_endpoint = SignalingProviderEndpoint::new(
        Endpoint::new(endpoint.clone()),
        SignalingEndpointSecurity::ProductionTls,
    );
    provider_endpoint.trust_fingerprint =
        std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_TRUST_FINGERPRINT")
            .ok()
            .or_else(|| {
                Some(discrypt_rendezvous_trust_fingerprint_for_endpoint(
                    &endpoint,
                ))
            });
    Ok(SignalingAdapterProfile {
        profile_id: "public-discrypt-rendezvous-webrtc-datachannel-e2e".to_owned(),
        kind: SignalingAdapterKind::DiscryptQuicRendezvous,
        endpoints: vec![provider_endpoint],
        metadata_posture: discrypt_transport::ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public discrypt rendezvous",
            "explicit self-hosted rendezvous service; opaque WebRTC negotiation envelopes only",
        )?,
    })
}

async fn run_provider_signaled_webrtc_datachannel_roundtrip(
    profile: SignalingAdapterProfile,
) -> Result<(), TransportError> {
    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-webrtc-datachannel-e2e",
        ),
    )?;
    let ice = IceServerConfig::new(vec![Endpoint::new("stun:stun.l.google.com:19302")], vec![])?;
    let probe = probe_provider_webrtc_datachannel_roundtrip(
        profile,
        scope,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        ice,
    )
    .await?;
    assert!(probe.offerer_direct_path_ready);
    assert!(probe.answerer_direct_path_ready);
    assert!(probe.offerer_data_channel_open);
    assert!(probe.answerer_data_channel_open);
    assert!(probe.text_control_frame_roundtrip);
    assert!(probe.receipt_frame_roundtrip);
    assert_eq!(probe.receipt_frame_sha256.len(), 64);
    Ok(())
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn run_provider_signaled_role_split_text_runtime_roundtrip(
    profile: SignalingAdapterProfile,
) -> Result<(), TransportError> {
    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-role-split-text-runtime-e2e",
        ),
    )?;
    let ice = discrypt_transport::WebRtcNegotiationConfig::new(IceServerConfig::new(
        vec![Endpoint::new("stun:stun.l.google.com:19302")],
        vec![],
    )?);
    let bootstrap_secret = random_bytes::<32>();
    let random_entropy = random_bytes::<16>();
    let offerer_peer_id = SignalingPeerId::new("public-role-split-offerer")?;
    let answerer_peer_id = SignalingPeerId::new("public-role-split-answerer")?;
    let answerer_remote_peer_id = offerer_peer_id.clone();
    let answerer_local_peer_id = answerer_peer_id.clone();
    let answerer_profile = profile.clone();
    let answerer_scope = scope.clone();
    let answerer_bootstrap_secret = bootstrap_secret;
    let answerer_random_entropy = random_entropy;
    let answerer_ice = ice.clone();
    let answerer_task = tokio::spawn(async move {
        start_provider_webrtc_text_control_answer_runtime_with_answerer(
            answerer_profile,
            answerer_scope,
            &answerer_bootstrap_secret,
            &answerer_random_entropy,
            answerer_ice,
            answerer_local_peer_id,
            answerer_remote_peer_id,
            |frame| {
                let mut receipt = b"ciphertext:role-split-public-receipt:".to_vec();
                receipt.extend_from_slice(&frame);
                Ok(receipt)
            },
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;

    let offerer = start_provider_webrtc_text_control_offer_runtime(
        profile,
        scope,
        &bootstrap_secret,
        &random_entropy,
        ice,
        offerer_peer_id,
        answerer_peer_id,
    )
    .await?;
    let answerer = tokio::time::timeout(std::time::Duration::from_secs(20), answerer_task)
        .await
        .map_err(|_| {
            TransportError::Unavailable(
                "timed out waiting for public role-split answerer attach".to_owned(),
            )
        })?
        .map_err(|error| {
            TransportError::Unavailable(format!(
                "public role-split answerer task join failed: {error}"
            ))
        })??;
    assert_eq!(
        offerer.evidence().role,
        ProviderTextControlRuntimePeerRole::Offerer
    );
    assert_eq!(
        answerer.evidence().role,
        ProviderTextControlRuntimePeerRole::Answerer
    );
    assert!(offerer.evidence().direct_path_ready);
    assert!(answerer.evidence().direct_path_ready);
    assert!(offerer.evidence().data_channel_open);
    assert!(answerer.evidence().data_channel_open);

    let transport = offerer.transport();
    let frame = b"ciphertext:role-split-public-frame".to_vec();
    transport.send_text_control_frame(frame.clone()).await?;
    let receipt = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        transport.recv_text_control_frame(),
    )
    .await
    .map_err(|_| {
        TransportError::Unavailable(
            "timed out waiting for public role-split text/control receipt".to_owned(),
        )
    })??;
    assert!(receipt.starts_with(b"ciphertext:role-split-public-receipt:"));
    assert!(receipt.ends_with(&frame));

    offerer.close().await?;
    answerer.close().await?;
    Ok(())
}

#[cfg(feature = "mqtt-adapter")]
fn public_turn_config_from_env() -> Result<WebRtcNegotiationConfig, TransportError> {
    let endpoint = std::env::var("DISCRYPT_PUBLIC_TURN_ENDPOINT").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "set DISCRYPT_PUBLIC_TURN_ENDPOINT for public TURN relay-only E2E".to_owned(),
        )
    })?;
    let username = std::env::var("DISCRYPT_PUBLIC_TURN_USERNAME").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "set DISCRYPT_PUBLIC_TURN_USERNAME for credentialed public TURN relay-only E2E"
                .to_owned(),
        )
    })?;
    let credential = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "set DISCRYPT_PUBLIC_TURN_CREDENTIAL for credentialed public TURN relay-only E2E"
                .to_owned(),
        )
    })?;
    let credential_expires_at = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL_EXPIRES_AT").ok();
    let ice = IceServerConfig::new(
        Vec::new(),
        vec![TurnServerConfig::new(
            Endpoint::new(endpoint),
            Some(username),
            Some(credential),
            credential_expires_at,
        )],
    )?;
    let mut config = WebRtcNegotiationConfig::new(ice);
    config.ice_transport_policy = WebRtcIceTransportPolicy::RelayOnly;
    Ok(config)
}

#[cfg(feature = "mqtt-adapter")]
fn public_turn_artifact_path() -> PathBuf {
    std::env::var("DISCRYPT_PUBLIC_TURN_ARTIFACT_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            PathBuf::from("target/e2e/per-30-configured-turn-proof/public-turn-relay-only.json")
        })
}

#[cfg(feature = "mqtt-adapter")]
fn sha256_hex_for_label(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"discrypt-public-turn-proof-redacted-label-v1");
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

#[cfg(feature = "mqtt-adapter")]
fn redacted_turn_endpoint_label(endpoint: &str) -> String {
    let scheme = endpoint
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| !scheme.is_empty())
        .unwrap_or("turn");
    format!("{scheme}:sha256:{}", &sha256_hex_for_label(endpoint)[..16])
}

#[cfg(feature = "mqtt-adapter")]
fn write_public_turn_proof_artifact(
    probe: &discrypt_transport::ProviderWebRtcDataChannelProbe,
    turn_endpoint: &str,
) -> Result<PathBuf, TransportError> {
    let path = public_turn_artifact_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            TransportError::Unavailable(format!(
                "failed to create public TURN artifact directory: {error}"
            ))
        })?;
    }
    let offerer_relay_candidates = probe.offerer_local_relay_candidates_gathered
        + probe.offerer_remote_relay_candidates_applied;
    let answerer_relay_candidates = probe.answerer_local_relay_candidates_gathered
        + probe.answerer_remote_relay_candidates_applied;
    let artifact = json!({
        "schema_version": "discrypt.p3_t09.configured_turn_proof.v1",
        "issue": "PER-30 / P3-T09",
        "status": "passed",
        "proof_level": "env-gated public TURN relay-only provider-signaled WebRTC DataChannel harness",
        "adapter": probe.kind.canonical_name(),
        "provider_endpoint_label": probe.endpoint_label,
        "provider_role": "signaling/rendezvous only",
        "provider_visible_material": [
            "adapter endpoint label",
            "derived hashed rendezvous topic",
            "sealed WebRTC offer/answer/candidate envelopes"
        ],
        "provider_application_relay_used": false,
        "turn_endpoint_label": redacted_turn_endpoint_label(turn_endpoint),
        "turn_credentials": {
            "configured": true,
            "username_redacted": true,
            "credential_redacted": true
        },
        "route_policy": {
            "ice_transport_policy": "relay_only",
            "direct_candidates_allowed": false,
            "configured_turn_required": true,
            "turn_selected_by_policy": true
        },
        "route_evidence": {
            "offerer_data_channel_open": probe.offerer_data_channel_open,
            "answerer_data_channel_open": probe.answerer_data_channel_open,
            "offerer_configured_turn_servers": probe.offerer_configured_turn_servers,
            "answerer_configured_turn_servers": probe.answerer_configured_turn_servers,
            "offerer_turn_fallback_ready": probe.offerer_turn_fallback_ready,
            "answerer_turn_fallback_ready": probe.answerer_turn_fallback_ready,
            "offerer_relay_candidates": offerer_relay_candidates,
            "answerer_relay_candidates": answerer_relay_candidates,
            "text_control_frame_roundtrip": probe.text_control_frame_roundtrip,
            "receipt_frame_roundtrip": probe.receipt_frame_roundtrip,
            "text_control_frame_sha256": probe.text_control_frame_sha256,
            "receipt_frame_sha256": probe.receipt_frame_sha256
        },
        "redaction": {
            "raw_turn_endpoint_logged": false,
            "raw_turn_username_logged": false,
            "raw_turn_credential_logged": false,
            "raw_sdp_logged": false,
            "raw_ice_candidate_logged": false,
            "raw_text_control_payload_logged": false
        },
        "diagnostics": {
            "offerer_timeline": probe.offerer_diagnostic_timeline,
            "answerer_timeline": probe.answerer_diagnostic_timeline
        },
        "non_claims": [
            "not installed Tauri app production readiness",
            "not OpenMLS admission proof",
            "not voice/media microphone proof",
            "not provider application relay"
        ]
    });
    let bytes = serde_json::to_vec_pretty(&artifact).map_err(|error| {
        TransportError::Unavailable(format!("failed to serialize public TURN artifact: {error}"))
    })?;
    std::fs::write(&path, bytes).map_err(|error| {
        TransportError::Unavailable(format!("failed to write public TURN artifact: {error}"))
    })?;
    eprintln!("public TURN relay-only proof artifact: {}", path.display());
    Ok(path)
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
fn media_frame_probe_payload() -> Vec<u8> {
    let mut frame = Vec::with_capacity(320);
    frame.extend_from_slice(b"media-frame-ciphertext:v1:");
    frame.extend_from_slice(&random_bytes::<16>());
    frame.extend(std::iter::repeat_n(0xAA_u8, 280));
    frame
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
fn media_frame_receipt_probe_payload(request: &[u8]) -> Vec<u8> {
    let mut receipt = Vec::with_capacity(request.len() + 48);
    receipt.extend_from_slice(b"media-receipt-v1:");
    receipt.extend_from_slice(request);
    receipt
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]
async fn run_provider_signaled_webrtc_media_frame_roundtrip(
    profile: SignalingAdapterProfile,
) -> Result<(), TransportError> {
    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-webrtc-media-frame-e2e",
        ),
    )?;
    let request = media_frame_probe_payload();
    let receipt = media_frame_receipt_probe_payload(&request);
    let ice = IceServerConfig::new(vec![Endpoint::new("stun:stun.l.google.com:19302")], vec![])?;
    let probe = probe_provider_webrtc_datachannel_request_response_roundtrip(
        profile,
        scope,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        ice,
        request,
        receipt,
    )
    .await?;
    assert!(probe.offerer_direct_path_ready);
    assert!(probe.answerer_direct_path_ready);
    assert!(probe.offerer_data_channel_open);
    assert!(probe.answerer_data_channel_open);
    assert!(probe.text_control_frame_roundtrip);
    assert!(probe.receipt_frame_roundtrip);
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_signals_real_webrtc_datachannel_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping public Nostr WebRTC E2E; set DISCRYPT_PUBLIC_NOSTR_WEBRTC_E2E=1 to run"
        );
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
        .unwrap_or_else(|_| "wss://relay.damus.io".to_owned());
    run_provider_signaled_webrtc_datachannel_roundtrip(public_nostr_profile(endpoint)?).await
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_ipfs_signals_real_webrtc_datachannel_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_IPFS_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public IPFS/libp2p WebRTC E2E; set DISCRYPT_PUBLIC_IPFS_WEBRTC_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS to comma-separated direct topic-peer multiaddrs to run");
        return Ok(());
    }
    let endpoints = public_ipfs_direct_topic_peer_endpoints()?;
    run_provider_signaled_webrtc_datachannel_roundtrip(public_ipfs_profile(endpoints)?).await
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_quic_rendezvous_signals_real_webrtc_datachannel_roundtrip(
) -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Discrypt rendezvous WebRTC E2E; set DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_WEBRTC_E2E=1 and DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT").map_err(|_| {
        TransportError::SignalingAdapter(
            "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT is required for deployed Discrypt rendezvous WebRTC E2E"
                .to_owned(),
        )
    })?;
    run_provider_signaled_webrtc_datachannel_roundtrip(public_discrypt_rendezvous_profile(
        endpoint,
    )?)
    .await
}

#[cfg(feature = "mqtt-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_mqtt_signals_real_webrtc_datachannel_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_MQTT_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public MQTT WebRTC E2E; set DISCRYPT_PUBLIC_MQTT_WEBRTC_E2E=1 to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
        .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned());
    run_provider_signaled_webrtc_datachannel_roundtrip(public_mqtt_profile(endpoint)?).await
}

#[cfg(feature = "mqtt-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_mqtt_role_split_text_runtime_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_MQTT_ROLE_SPLIT_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public MQTT role-split runtime E2E; set DISCRYPT_PUBLIC_MQTT_ROLE_SPLIT_E2E=1 to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
        .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned());
    run_provider_signaled_role_split_text_runtime_roundtrip(public_mqtt_profile(endpoint)?).await
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_role_split_text_runtime_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_ROLE_SPLIT_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Nostr role-split runtime E2E; set DISCRYPT_PUBLIC_NOSTR_ROLE_SPLIT_E2E=1 to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
        .unwrap_or_else(|_| "wss://nos.lol".to_owned());
    run_provider_signaled_role_split_text_runtime_roundtrip(public_nostr_profile(endpoint)?).await
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_ipfs_role_split_text_runtime_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_IPFS_ROLE_SPLIT_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public IPFS/libp2p role-split runtime E2E; set DISCRYPT_PUBLIC_IPFS_ROLE_SPLIT_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS to run");
        return Ok(());
    }
    let endpoints = public_ipfs_direct_topic_peer_endpoints()?;
    run_provider_signaled_role_split_text_runtime_roundtrip(public_ipfs_profile(endpoints)?).await
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_quic_rendezvous_role_split_text_runtime_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ROLE_SPLIT_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Discrypt rendezvous role-split runtime E2E; set DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ROLE_SPLIT_E2E=1 and DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT").map_err(|_| {
        TransportError::SignalingAdapter(
            "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT is required for deployed Discrypt rendezvous role-split runtime E2E"
                .to_owned(),
        )
    })?;
    run_provider_signaled_role_split_text_runtime_roundtrip(public_discrypt_rendezvous_profile(
        endpoint,
    )?)
    .await
}

#[cfg(feature = "mqtt-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_mqtt_signals_real_webrtc_media_frame_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_MQTT_MEDIA_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping media-frame transport gate; set DISCRYPT_PUBLIC_MQTT_MEDIA_WEBRTC_E2E=1 to run"
        );
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
        .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned());
    run_provider_signaled_webrtc_media_frame_roundtrip(public_mqtt_profile(endpoint)?).await
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_signals_real_webrtc_media_frame_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_MEDIA_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping public Nostr media-frame transport gate; set DISCRYPT_PUBLIC_NOSTR_MEDIA_WEBRTC_E2E=1 to run"
        );
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
        .unwrap_or_else(|_| "wss://nos.lol".to_owned());
    run_provider_signaled_webrtc_media_frame_roundtrip(public_nostr_profile(endpoint)?).await
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_ipfs_signals_real_webrtc_media_frame_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_IPFS_MEDIA_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public IPFS/libp2p media-frame WebRTC E2E; set DISCRYPT_PUBLIC_IPFS_MEDIA_WEBRTC_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS to comma-separated direct topic-peer multiaddrs to run");
        return Ok(());
    }
    let endpoints = public_ipfs_direct_topic_peer_endpoints()?;
    run_provider_signaled_webrtc_media_frame_roundtrip(public_ipfs_profile(endpoints)?).await
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_quic_rendezvous_signals_real_webrtc_media_frame_roundtrip(
) -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_MEDIA_WEBRTC_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Discrypt rendezvous media-frame WebRTC E2E; set DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_MEDIA_WEBRTC_E2E=1 and DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... to run");
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT").map_err(|_| {
        TransportError::SignalingAdapter(
            "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT is required for deployed Discrypt rendezvous media-frame E2E"
                .to_owned(),
        )
    })?;
    run_provider_signaled_webrtc_media_frame_roundtrip(public_discrypt_rendezvous_profile(
        endpoint,
    )?)
    .await
}

#[cfg(feature = "mqtt-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_mqtt_relay_only_turn_fallback_roundtrip_when_configured(
) -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_TURN_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping public TURN relay-only E2E; set DISCRYPT_PUBLIC_TURN_E2E=1 with TURN envs to run"
        );
        return Ok(());
    }
    let endpoint = std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
        .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned());
    let turn_endpoint = std::env::var("DISCRYPT_PUBLIC_TURN_ENDPOINT").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "set DISCRYPT_PUBLIC_TURN_ENDPOINT for public TURN relay-only E2E".to_owned(),
        )
    })?;
    let turn_config = public_turn_config_from_env()?;
    assert_eq!(
        turn_config.ice_transport_policy,
        WebRtcIceTransportPolicy::RelayOnly
    );
    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-turn-relay-only-e2e",
        ),
    )?;
    let probe = probe_provider_webrtc_datachannel_request_response_with_config(
        public_mqtt_profile(endpoint)?,
        scope,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        turn_config,
        b"ciphertext:relay-only-turn-request".to_vec(),
        b"ciphertext:relay-only-turn-receipt".to_vec(),
    )
    .await?;
    assert!(probe.offerer_data_channel_open);
    assert!(probe.answerer_data_channel_open);
    assert!(probe.text_control_frame_roundtrip);
    assert!(probe.receipt_frame_roundtrip);
    assert!(probe.offerer_configured_turn_servers > 0);
    assert!(probe.answerer_configured_turn_servers > 0);
    assert!(probe.offerer_turn_fallback_ready);
    assert!(probe.answerer_turn_fallback_ready);
    assert!(
        probe.offerer_local_relay_candidates_gathered
            + probe.offerer_remote_relay_candidates_applied
            > 0
    );
    assert!(
        probe.answerer_local_relay_candidates_gathered
            + probe.answerer_remote_relay_candidates_applied
            > 0
    );
    write_public_turn_proof_artifact(&probe, &turn_endpoint)?;
    Ok(())
}
