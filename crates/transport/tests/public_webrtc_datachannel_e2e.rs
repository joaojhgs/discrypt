#![cfg(any(feature = "mqtt-adapter", feature = "nostr-adapter"))]

use discrypt_transport::{
    derive_scope_commitment, probe_provider_webrtc_datachannel_roundtrip, AdapterTrustLabel,
    ConnectivityScopeLevel, ConversationScope, Endpoint, IceServerConfig,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingProviderEndpoint, TransportError,
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
