#![cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]

#[cfg(feature = "ipfs-pubsub-adapter")]
use discrypt_transport::IpfsPubsubProviderAdapter;
#[cfg(feature = "mqtt-adapter")]
use discrypt_transport::MqttProviderAdapter;
#[cfg(feature = "nostr-adapter")]
use discrypt_transport::NostrProviderAdapter;
use discrypt_transport::{
    derive_scope_commitment, AdapterSession, AdapterTrustLabel, ConnectivityScopeLevel,
    ConversationScope, Endpoint, OpaqueSignalingPayload, ProviderMetadataPosture,
    RendezvousCapability, RendezvousRoom, SealedWebRtcNegotiationPayload, SignalingAdapter,
    SignalingAdapterCapabilities, SignalingAdapterKind, SignalingAdapterProfile,
    SignalingEndpointSecurity, SignalingPeerId, SignalingProviderEndpoint, TransportError,
    WebRtcNegotiationPayloadKind,
};
use rand::RngCore;
use tokio::time::{sleep, Duration, Instant};

#[cfg(feature = "mqtt-adapter")]
fn public_mqtt_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-mqtt-e2e".to_owned(),
        kind: SignalingAdapterKind::Mqtt,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new("public mqtt", "public broker; opaque envelopes only")?,
    })
}

#[cfg(feature = "nostr-adapter")]
fn public_nostr_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-nostr-e2e".to_owned(),
        kind: SignalingAdapterKind::Nostr,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new("public nostr", "public relay; opaque envelopes only")?,
    })
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn public_ipfs_profile(endpoints: Vec<String>) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "public-ipfs-pubsub-e2e".to_owned(),
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
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public ipfs_pubsub",
            "public libp2p bootstrap peers; opaque envelopes only",
        )?,
    })
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

async fn wait_for<T, Fut>(mut poll: impl FnMut() -> Fut) -> Result<T, TransportError>
where
    Fut: std::future::Future<Output = Result<Option<T>, TransportError>>,
{
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(value) = poll().await? {
            return Ok(value);
        }
        if Instant::now() >= deadline {
            return Err(TransportError::SignalingAdapter(
                "timed out waiting for public signaling e2e message".to_owned(),
            ));
        }
        sleep(Duration::from_millis(300)).await;
    }
}

#[cfg(feature = "mqtt-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_mqtt_two_peer_presence_signal_and_control_roundtrip() -> Result<(), TransportError>
{
    if std::env::var("DISCRYPT_PUBLIC_SIGNALING_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public MQTT E2E; set DISCRYPT_PUBLIC_SIGNALING_E2E=1 to run");
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
        .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned());
    let profile = public_mqtt_profile(endpoint)?;
    let adapter = MqttProviderAdapter;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile).await?;

    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(ConnectivityScopeLevel::Dm, &scope_secret, "public-mqtt-e2e"),
    )?;
    let capability = RendezvousCapability::derive(
        scope.clone(),
        SignalingAdapterKind::Mqtt,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        120,
        ProviderMetadataPosture::HashedTopic,
        AdapterTrustLabel::new("public mqtt", "hashed topic and opaque payloads")?,
    )?;

    let alice = SignalingPeerId::new("alice-device")?;
    let bob = SignalingPeerId::new("bob-device")?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session.join(scope, capability, bob.clone()).await?;

    sleep(Duration::from_secs(1)).await;
    alice_room
        .publish_presence(
            OpaqueSignalingPayload::new(b"sealed-presence-alice".to_vec())?,
            120,
        )
        .await?;
    let bob_presence = wait_for(|| async {
        let events = bob_room.subscribe_presence().await?;
        Ok(events.into_iter().find(|event| event.peer_id == alice))
    })
    .await?;
    assert_eq!(bob_presence.ttl_seconds, 120);

    let offer = SealedWebRtcNegotiationPayload {
        version: 1,
        kind: WebRtcNegotiationPayloadKind::Offer,
        nonce: random_bytes::<12>(),
        ciphertext: b"sealed-offer-ciphertext".to_vec(),
    };
    alice_room.send_signal(bob.clone(), offer.clone()).await?;
    let received_offer = wait_for(|| async {
        let signals = bob_room.take_signals().await?;
        Ok(signals
            .into_iter()
            .find(|signal| signal.from_peer == alice && signal.to_peer == bob))
    })
    .await?;
    assert_eq!(received_offer.payload, offer);

    bob_room
        .broadcast_control(OpaqueSignalingPayload::new(b"sealed-control-bob".to_vec())?)
        .await?;
    let received_control = wait_for(|| async {
        let controls = alice_room.take_control_payloads().await?;
        Ok(controls
            .into_iter()
            .find(|control| control.from_peer == bob))
    })
    .await?;
    assert_eq!(
        received_control.payload.bytes,
        b"sealed-control-bob".to_vec()
    );

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_two_peer_presence_signal_and_control_roundtrip() -> Result<(), TransportError>
{
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Nostr E2E; set DISCRYPT_PUBLIC_NOSTR_E2E=1 to run");
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
        .unwrap_or_else(|_| "wss://relay.damus.io".to_owned());
    let profile = public_nostr_profile(endpoint)?;
    let adapter = NostrProviderAdapter;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile).await?;

    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-nostr-e2e",
        ),
    )?;
    let capability = RendezvousCapability::derive(
        scope.clone(),
        SignalingAdapterKind::Nostr,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        120,
        ProviderMetadataPosture::HashedTopic,
        AdapterTrustLabel::new("public nostr", "hashed topic and opaque payloads")?,
    )?;

    let alice = SignalingPeerId::new("alice-device")?;
    let bob = SignalingPeerId::new("bob-device")?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session.join(scope, capability, bob.clone()).await?;

    sleep(Duration::from_secs(2)).await;
    alice_room
        .publish_presence(
            OpaqueSignalingPayload::new(b"sealed-presence-alice".to_vec())?,
            120,
        )
        .await?;
    let bob_presence = wait_for(|| async {
        let events = bob_room.subscribe_presence().await?;
        Ok(events.into_iter().find(|event| event.peer_id == alice))
    })
    .await?;
    assert_eq!(bob_presence.ttl_seconds, 120);

    let offer = SealedWebRtcNegotiationPayload {
        version: 1,
        kind: WebRtcNegotiationPayloadKind::Offer,
        nonce: random_bytes::<12>(),
        ciphertext: b"sealed-nostr-offer-ciphertext".to_vec(),
    };
    alice_room.send_signal(bob.clone(), offer.clone()).await?;
    let received_offer = wait_for(|| async {
        let signals = bob_room.take_signals().await?;
        Ok(signals
            .into_iter()
            .find(|signal| signal.from_peer == alice && signal.to_peer == bob))
    })
    .await?;
    assert_eq!(received_offer.payload, offer);

    bob_room
        .broadcast_control(OpaqueSignalingPayload::new(b"sealed-control-bob".to_vec())?)
        .await?;
    let received_control = wait_for(|| async {
        let controls = alice_room.take_control_payloads().await?;
        Ok(controls
            .into_iter()
            .find(|control| control.from_peer == bob))
    })
    .await?;
    assert_eq!(
        received_control.payload.bytes,
        b"sealed-control-bob".to_vec()
    );

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_ipfs_two_peer_signaling_smoke() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_IPFS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public IPFS/libp2p E2E; set DISCRYPT_PUBLIC_IPFS_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS to comma-separated multiaddrs to run");
        return Ok(());
    }

    let endpoints = std::env::var("DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS")
        .map_err(|_| {
            TransportError::SignalingAdapter(
                "DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS is required for public IPFS E2E"
                    .to_owned(),
            )
        })?
        .split(',')
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if endpoints.is_empty() {
        return Err(TransportError::SignalingAdapter(
            "public IPFS E2E requires at least one bootstrap multiaddr".to_owned(),
        ));
    }

    let profile = public_ipfs_profile(endpoints)?;
    let adapter = IpfsPubsubProviderAdapter;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile).await?;

    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(ConnectivityScopeLevel::Dm, &scope_secret, "public-ipfs-e2e"),
    )?;
    let capability = RendezvousCapability::derive(
        scope.clone(),
        SignalingAdapterKind::IpfsPubsub,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        120,
        ProviderMetadataPosture::HashedTopic,
        AdapterTrustLabel::new("public ipfs_pubsub", "hashed topic and opaque payloads")?,
    )?;

    let alice = SignalingPeerId::new("alice-device")?;
    let bob = SignalingPeerId::new("bob-device")?;
    let alice_room = alice_session
        .join(scope.clone(), capability.clone(), alice.clone())
        .await?;
    let bob_room = bob_session.join(scope, capability, bob.clone()).await?;

    sleep(Duration::from_secs(3)).await;
    alice_room
        .publish_presence(
            OpaqueSignalingPayload::new(b"sealed-presence-alice".to_vec())?,
            120,
        )
        .await?;
    let bob_presence = wait_for(|| async {
        let events = bob_room.subscribe_presence().await?;
        Ok(events.into_iter().find(|event| event.peer_id == alice))
    })
    .await?;
    assert_eq!(bob_presence.ttl_seconds, 120);

    let offer = SealedWebRtcNegotiationPayload {
        version: 1,
        kind: WebRtcNegotiationPayloadKind::Offer,
        nonce: random_bytes::<12>(),
        ciphertext: b"sealed-ipfs-offer-ciphertext".to_vec(),
    };
    alice_room.send_signal(bob.clone(), offer.clone()).await?;
    let received_offer = wait_for(|| async {
        let signals = bob_room.take_signals().await?;
        Ok(signals
            .into_iter()
            .find(|signal| signal.from_peer == alice && signal.to_peer == bob))
    })
    .await?;
    assert_eq!(received_offer.payload, offer);

    bob_room
        .broadcast_control(OpaqueSignalingPayload::new(b"sealed-control-bob".to_vec())?)
        .await?;
    let received_control = wait_for(|| async {
        let controls = alice_room.take_control_payloads().await?;
        Ok(controls
            .into_iter()
            .find(|control| control.from_peer == bob))
    })
    .await?;
    assert_eq!(
        received_control.payload.bytes,
        b"sealed-control-bob".to_vec()
    );

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}
