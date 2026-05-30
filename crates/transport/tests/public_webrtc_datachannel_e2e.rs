#![cfg(any(feature = "mqtt-adapter", feature = "nostr-adapter"))]

#[cfg(feature = "mqtt-adapter")]
use discrypt_transport::MqttProviderAdapter;
#[cfg(feature = "nostr-adapter")]
use discrypt_transport::NostrProviderAdapter;
use discrypt_transport::{
    derive_scope_commitment, AdapterSession, AdapterTrustLabel, ConnectivityScopeLevel,
    ConversationScope, Endpoint, IceServerConfig, RendezvousCapability, RendezvousRoom,
    SignalingAdapter, SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingPeerId, SignalingProviderEndpoint,
    TextControlDataTransport, TransportError, WebRtcNegotiationConfig,
    WebRtcNegotiationPayloadKind, WebRtcNegotiationSealer, WebRtcNegotiator,
};
use rand::RngCore;
use tokio::time::{sleep, timeout, Duration, Instant};

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

async fn run_provider_signaled_webrtc_datachannel_roundtrip<A>(
    adapter: A,
    profile: SignalingAdapterProfile,
) -> Result<(), TransportError>
where
    A: SignalingAdapter,
{
    let kind = profile.kind;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile).await?;

    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-webrtc-datachannel-e2e",
        ),
    )?;
    let capability = RendezvousCapability::derive(
        scope.clone(),
        kind,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        120,
        discrypt_transport::ProviderMetadataPosture::HashedTopic,
        AdapterTrustLabel::new(
            kind.canonical_name(),
            "hashed topic and opaque WebRTC negotiation payloads",
        )?,
    )?;
    let alice = SignalingPeerId::new("alice-device")?;
    let bob = SignalingPeerId::new("bob-device")?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session.join(scope, capability, bob.clone()).await?;

    sleep(Duration::from_secs(1)).await;

    let ice = IceServerConfig::new(vec![Endpoint::new("stun:127.0.0.1:3478")], vec![])?;
    let alice_webrtc = WebRtcNegotiator::new(WebRtcNegotiationConfig::new(ice.clone())).await?;
    let bob_webrtc = WebRtcNegotiator::new(WebRtcNegotiationConfig::new(ice)).await?;
    let sealer = WebRtcNegotiationSealer::new([0x9d; 32]);

    let offer = alice_webrtc.create_offer().await?;
    let sealed_offer = sealer.seal_description(&offer)?;
    let opaque_offer = sealed_offer.to_opaque_bytes()?;
    if opaque_offer.windows(3).any(|window| window == b"v=0") {
        return Err(TransportError::PlaintextLeak);
    }
    alice_room.send_signal(bob.clone(), sealed_offer).await?;

    let mut answer_applied = false;
    let deadline = Instant::now() + Duration::from_secs(45);
    while Instant::now() < deadline {
        for signal in bob_room.take_signals().await? {
            if signal.from_peer != alice || signal.to_peer != bob {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Offer => {
                    let offer = sealer.open_description(&signal.payload)?;
                    let answer = bob_webrtc.create_answer(offer).await?;
                    bob_room
                        .send_signal(alice.clone(), sealer.seal_description(&answer)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    bob_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for signal in alice_room.take_signals().await? {
            if signal.from_peer != bob || signal.to_peer != alice {
                continue;
            }
            match signal.payload.kind {
                WebRtcNegotiationPayloadKind::Answer if !answer_applied => {
                    alice_webrtc
                        .accept_answer(sealer.open_description(&signal.payload)?)
                        .await?;
                    answer_applied = true;
                }
                WebRtcNegotiationPayloadKind::Candidate => {
                    alice_webrtc
                        .add_remote_candidate(sealer.open_candidate(&signal.payload)?)
                        .await?;
                }
                WebRtcNegotiationPayloadKind::Offer | WebRtcNegotiationPayloadKind::Answer => {}
            }
        }

        for candidate in alice_webrtc.drain_local_candidates().await {
            alice_room
                .send_signal(bob.clone(), sealer.seal_candidate(&candidate)?)
                .await?;
        }
        for candidate in bob_webrtc.drain_local_candidates().await {
            bob_room
                .send_signal(alice.clone(), sealer.seal_candidate(&candidate)?)
                .await?;
        }

        if answer_applied
            && alice_webrtc.direct_path_metrics().await.direct_path_ready
            && bob_webrtc.direct_path_metrics().await.direct_path_ready
        {
            alice_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            bob_webrtc
                .wait_text_control_transport_ready(Duration::from_secs(5))
                .await?;
            break;
        }
        sleep(Duration::from_millis(250)).await;
    }

    if !alice_webrtc.text_control_transport_metrics().await.open
        || !bob_webrtc.text_control_transport_metrics().await.open
    {
        return Err(TransportError::Unavailable(format!(
            "provider-signaled WebRTC data channel did not open: alice={:?} bob={:?}",
            alice_webrtc.direct_path_metrics().await,
            bob_webrtc.direct_path_metrics().await
        )));
    }

    let frame = b"ciphertext:public-provider-signaled-webrtc-text-frame:v1".to_vec();
    alice_webrtc.send_text_control_frame(frame.clone()).await?;
    let received = timeout(Duration::from_secs(5), bob_webrtc.recv_text_control_frame())
        .await
        .map_err(|_| {
            TransportError::Unavailable("timed out receiving WebRTC data frame".to_owned())
        })??;
    assert_eq!(received, frame);

    alice_webrtc.tear_down().await?;
    bob_webrtc.tear_down().await?;
    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
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
    run_provider_signaled_webrtc_datachannel_roundtrip(
        NostrProviderAdapter,
        public_nostr_profile(endpoint)?,
    )
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
    run_provider_signaled_webrtc_datachannel_roundtrip(
        MqttProviderAdapter,
        public_mqtt_profile(endpoint)?,
    )
    .await
}
