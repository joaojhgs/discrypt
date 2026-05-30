#![cfg(any(feature = "mqtt-adapter", feature = "nostr-adapter"))]

use discrypt_transport::{
    derive_scope_commitment, probe_provider_webrtc_datachannel_roundtrip, AdapterTrustLabel,
    ConnectivityScopeLevel, ConversationScope, Endpoint, IceServerConfig,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingProviderEndpoint, TransportError,
};
#[cfg(feature = "mqtt-adapter")]
use discrypt_transport::{
    probe_provider_webrtc_datachannel_request_response_roundtrip,
    probe_provider_webrtc_datachannel_request_response_with_config, TurnServerConfig,
    WebRtcIceTransportPolicy, WebRtcNegotiationConfig,
};
use rand::RngCore;

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

#[cfg(feature = "mqtt-adapter")]
fn public_turn_config_from_env() -> Result<WebRtcNegotiationConfig, TransportError> {
    let endpoint = std::env::var("DISCRYPT_PUBLIC_TURN_ENDPOINT").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "set DISCRYPT_PUBLIC_TURN_ENDPOINT for public TURN relay-only E2E".to_owned(),
        )
    })?;
    let username = std::env::var("DISCRYPT_PUBLIC_TURN_USERNAME").ok();
    let credential = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL").ok();
    let credential_expires_at = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL_EXPIRES_AT").ok();
    let ice = IceServerConfig::new(
        Vec::new(),
        vec![TurnServerConfig::new(
            Endpoint::new(endpoint),
            username,
            credential,
            credential_expires_at,
        )],
    )?;
    let mut config = WebRtcNegotiationConfig::new(ice);
    config.ice_transport_policy = WebRtcIceTransportPolicy::RelayOnly;
    Ok(config)
}

#[cfg(feature = "mqtt-adapter")]
fn media_frame_probe_payload() -> Vec<u8> {
    let mut frame = Vec::with_capacity(320);
    frame.extend_from_slice(b"media-frame-ciphertext:v1:");
    frame.extend_from_slice(&random_bytes::<16>());
    frame.extend(std::iter::repeat_n(0xAA_u8, 280));
    frame
}

#[cfg(feature = "mqtt-adapter")]
fn media_frame_receipt_probe_payload(request: &[u8]) -> Vec<u8> {
    let mut receipt = Vec::with_capacity(request.len() + 48);
    receipt.extend_from_slice(b"media-receipt-v1:");
    receipt.extend_from_slice(request);
    receipt
}

#[cfg(feature = "mqtt-adapter")]
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
        public_turn_config_from_env()?,
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
    Ok(())
}
