#![cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter",
    feature = "discrypt-quic-rendezvous-adapter"
))]

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
use discrypt_transport::probe_provider_adapter_roundtrip;
#[cfg(feature = "ipfs-pubsub-adapter")]
use discrypt_transport::IpfsPubsubProviderAdapter;
#[cfg(feature = "mqtt-adapter")]
use discrypt_transport::MqttProviderAdapter;
#[cfg(feature = "nostr-adapter")]
use discrypt_transport::NostrProviderAdapter;
use discrypt_transport::{
    derive_scope_commitment, AdapterTrustLabel, ConnectivityScopeLevel, ConversationScope,
    Endpoint, ProviderMetadataPosture, SignalingAdapterCapabilities, SignalingAdapterKind,
    SignalingAdapterProfile, SignalingEndpointSecurity, SignalingProviderEndpoint, TransportError,
};
#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use discrypt_transport::{
    AdapterSession, OpaqueSignalingPayload, RendezvousCapability, RendezvousRoom,
    SealedWebRtcNegotiationPayload, SignalingAdapter, SignalingPeerId,
    WebRtcNegotiationPayloadKind,
};
#[cfg(feature = "nostr-adapter")]
use futures::{SinkExt, StreamExt};
use rand::RngCore;
#[cfg(feature = "nostr-adapter")]
use serde_json::Value;
#[cfg(feature = "nostr-adapter")]
use tokio::net::{TcpListener, TcpStream};
#[cfg(feature = "nostr-adapter")]
use tokio::sync::broadcast;
#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
use tokio::time::{sleep, Duration, Instant};
#[cfg(feature = "nostr-adapter")]
use tokio_tungstenite::tungstenite::Message;

#[cfg(feature = "mqtt-adapter")]
fn public_mqtt_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    let security = if endpoint.starts_with("mqtt://127.0.0.1")
        || endpoint.starts_with("mqtt://[::1]")
        || endpoint.starts_with("ws://127.0.0.1")
        || endpoint.starts_with("ws://[::1]")
    {
        SignalingEndpointSecurity::LocalDevLoopback
    } else {
        SignalingEndpointSecurity::ProductionTls
    };
    let mut provider_endpoint = SignalingProviderEndpoint::new(Endpoint::new(endpoint), security);
    provider_endpoint.max_message_bytes =
        Some(discrypt_transport::DEFAULT_PROVIDER_MAX_MESSAGE_BYTES);
    Ok(SignalingAdapterProfile {
        profile_id: "public-mqtt-e2e".to_owned(),
        kind: SignalingAdapterKind::Mqtt,
        endpoints: vec![provider_endpoint],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new("public mqtt", "public broker; opaque envelopes only")?,
    })
}

#[cfg(feature = "nostr-adapter")]
fn public_nostr_profile(
    endpoints: Vec<String>,
    profile_id: &str,
    trust_posture: &str,
) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: profile_id.to_owned(),
        kind: SignalingAdapterKind::Nostr,
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
        trust_label: AdapterTrustLabel::new("public nostr", trust_posture)?,
    })
}

#[cfg(feature = "nostr-adapter")]
fn comma_separated_endpoints(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(feature = "nostr-adapter")]
struct LocalNostrRelay {
    endpoint: String,
    handle: tokio::task::JoinHandle<()>,
}

#[cfg(feature = "nostr-adapter")]
impl Drop for LocalNostrRelay {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

#[cfg(feature = "nostr-adapter")]
async fn spawn_local_nostr_relay() -> Result<LocalNostrRelay, TransportError> {
    let listener = TcpListener::bind("127.0.0.1:0").await.map_err(|err| {
        TransportError::SignalingAdapter(format!("bind local Nostr relay: {err}"))
    })?;
    let endpoint = format!(
        "ws://{}",
        listener.local_addr().map_err(|err| {
            TransportError::SignalingAdapter(format!("read local Nostr relay address: {err}"))
        })?
    );
    let (events, _) = broadcast::channel::<Value>(128);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                break;
            };
            let events = events.clone();
            tokio::spawn(async move {
                if let Err(error) = handle_local_nostr_relay_connection(stream, events).await {
                    eprintln!("local Nostr relay connection closed: {error}");
                }
            });
        }
    });
    Ok(LocalNostrRelay { endpoint, handle })
}

#[cfg(feature = "nostr-adapter")]
async fn handle_local_nostr_relay_connection(
    stream: TcpStream,
    events: broadcast::Sender<Value>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let websocket = tokio_tungstenite::accept_async(stream).await?;
    let (mut writer, mut reader) = websocket.split();
    let mut event_rx = events.subscribe();
    let mut subscriptions = Vec::<String>::new();

    loop {
        tokio::select! {
            message = reader.next() => {
                let Some(message) = message else {
                    break;
                };
                let Message::Text(text) = message? else {
                    continue;
                };
                let Value::Array(parts) = serde_json::from_str::<Value>(&text)? else {
                    continue;
                };
                let Some(command) = parts.first().and_then(Value::as_str) else {
                    continue;
                };
                match command {
                    "REQ" => {
                        if let Some(subscription_id) = parts.get(1).and_then(Value::as_str) {
                            subscriptions.push(subscription_id.to_owned());
                            let eose = serde_json::json!(["EOSE", subscription_id]);
                            writer.send(Message::Text(eose.to_string().into())).await?;
                        }
                    }
                    "EVENT" => {
                        if let Some(event) = parts.get(1).cloned() {
                            let event_id = event.get("id").and_then(Value::as_str).unwrap_or("");
                            let ok = serde_json::json!(["OK", event_id, true, ""]);
                            writer.send(Message::Text(ok.to_string().into())).await?;
                            let _ = events.send(event);
                        }
                    }
                    "CLOSE" => {
                        if let Some(subscription_id) = parts.get(1).and_then(Value::as_str) {
                            subscriptions.retain(|id| id != subscription_id);
                        }
                    }
                    _ => {}
                }
            }
            event = event_rx.recv(), if !subscriptions.is_empty() => {
                match event {
                    Ok(event) => {
                        for subscription_id in &subscriptions {
                            let relay_event = serde_json::json!(["EVENT", subscription_id, event]);
                            writer.send(Message::Text(relay_event.to_string().into())).await?;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
fn local_nostr_profile(endpoint: String) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "local-nostr-relay-e2e".to_owned(),
        kind: SignalingAdapterKind::Nostr,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint),
            SignalingEndpointSecurity::LocalDevLoopback,
        )],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "local nostr",
            "loopback relay; opaque envelopes only",
        )?,
    })
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
        profile_id: "public-discrypt-rendezvous-e2e".to_owned(),
        kind: SignalingAdapterKind::DiscryptQuicRendezvous,
        endpoints: vec![provider_endpoint],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "public discrypt rendezvous",
            "explicit self-hosted rendezvous service; opaque envelopes only",
        )?,
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

#[cfg(feature = "ipfs-pubsub-adapter")]
fn validate_public_ipfs_direct_topic_peer_endpoints(
    endpoints: &[String],
) -> Result<(), TransportError> {
    if endpoints.is_empty() {
        return Err(TransportError::SignalingAdapter(
            "public IPFS E2E requires at least one bootstrap multiaddr".to_owned(),
        ));
    }
    if endpoints.iter().any(|endpoint| !endpoint.contains("/p2p/")) {
        return Err(TransportError::InvalidConnectivityPolicy(
            "public IPFS E2E requires explicit direct topic-peer multiaddrs; DNS/bootstrap or bare dialable endpoints are not accepted".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(feature = "ipfs-pubsub-adapter")]
fn public_ipfs_direct_topic_peer_endpoints() -> Result<Vec<String>, TransportError> {
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
    validate_public_ipfs_direct_topic_peer_endpoints(&endpoints)?;
    Ok(endpoints)
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[test]
fn public_ipfs_direct_topic_peer_validation_rejects_non_topic_peer_endpoints() {
    let error = validate_public_ipfs_direct_topic_peer_endpoints(&[String::from(
        "/ip4/203.0.113.10/tcp/4001",
    )])
    .expect_err("bare bootstrap endpoint must be rejected");
    assert!(
        format!("{error}").contains("direct topic-peer multiaddrs"),
        "expected direct topic-peer blocker, got: {error}"
    );
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[test]
fn public_ipfs_direct_topic_peer_validation_accepts_explicit_peer_multiaddrs() {
    validate_public_ipfs_direct_topic_peer_endpoints(&[String::from(
        "/ip4/203.0.113.10/tcp/4001/p2p/QmNnooDu7bfjPFoTZYxMNLWUQJyrVwtbZg5gBMjTezGAJN",
    )])
    .expect("direct topic-peer endpoint should be accepted");
}

fn random_bytes<const N: usize>() -> [u8; N] {
    let mut bytes = [0_u8; N];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes
}

#[cfg(any(
    feature = "mqtt-adapter",
    feature = "nostr-adapter",
    feature = "ipfs-pubsub-adapter"
))]
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
async fn public_mqtt_two_peer_presence_and_signal_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_MQTT_E2E").as_deref() != Ok("1")
        && std::env::var("DISCRYPT_PUBLIC_SIGNALING_E2E").as_deref() != Ok("1")
    {
        eprintln!("skipping public MQTT E2E; set DISCRYPT_PUBLIC_MQTT_E2E=1 or legacy DISCRYPT_PUBLIC_SIGNALING_E2E=1 to run");
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

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
async fn run_public_nostr_two_peer_roundtrip(
    profile: SignalingAdapterProfile,
    scope_label: &str,
    trust_posture: &str,
) -> Result<(), TransportError> {
    let adapter = NostrProviderAdapter;
    let alice_session = adapter.connect(profile.clone()).await?;
    let bob_session = adapter.connect(profile).await?;

    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(ConnectivityScopeLevel::Dm, &scope_secret, scope_label),
    )?;
    let capability = RendezvousCapability::derive(
        scope.clone(),
        SignalingAdapterKind::Nostr,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
        120,
        ProviderMetadataPosture::HashedTopic,
        AdapterTrustLabel::new("public nostr", trust_posture)?,
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

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_nostr_relay_presence_and_signal_roundtrip() -> Result<(), TransportError> {
    let relay = spawn_local_nostr_relay().await?;
    let profile = local_nostr_profile(relay.endpoint.clone())?;
    run_public_nostr_two_peer_roundtrip(
        profile,
        "local-nostr-relay-e2e",
        "loopback relay with hashed topic and opaque payloads",
    )
    .await
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_two_peer_presence_and_signal_roundtrip() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Nostr E2E; set DISCRYPT_PUBLIC_NOSTR_E2E=1 to run");
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
        .unwrap_or_else(|_| "wss://relay.damus.io".to_owned());
    let profile = public_nostr_profile(
        vec![endpoint],
        "public-nostr-e2e",
        "public relay; opaque envelopes only",
    )?;
    run_public_nostr_two_peer_roundtrip(
        profile,
        "public-nostr-e2e",
        "hashed topic and opaque payloads",
    )
    .await
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_blocked_relay_maps_to_auth_required() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_REJECTION_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Nostr rejection evidence; set DISCRYPT_PUBLIC_NOSTR_REJECTION_E2E=1 to run");
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_NOSTR_REJECTION_ENDPOINT")
        .unwrap_or_else(|_| "wss://nostr.oxtr.dev".to_owned());
    let profile = public_nostr_profile(
        vec![endpoint],
        "public-nostr-rejection-e2e",
        "public relay expected to reject custom Discrypt events; typed failure evidence only",
    )?;
    let error = run_public_nostr_two_peer_roundtrip(
        profile,
        "public-nostr-rejection-e2e",
        "hashed topic and opaque payloads; rejection path must stay redacted",
    )
    .await
    .expect_err("configured rejection relay unexpectedly accepted the Discrypt event");
    let message = format!("{error}");
    assert!(
        message.contains("failure_class=provider_auth_required"),
        "expected provider_auth_required failure class, got: {message}"
    );
    assert!(
        message.contains("health_state=ProviderAuthRequired"),
        "expected ProviderAuthRequired health state, got: {message}"
    );
    assert!(
        !message.contains("sealed-nostr-offer-ciphertext"),
        "typed public rejection evidence must not leak opaque payload bytes: {message}"
    );
    Ok(())
}

#[cfg(feature = "nostr-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_nostr_multi_relay_degraded_fallback_soak() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public Nostr multi-relay soak; set DISCRYPT_PUBLIC_NOSTR_MULTI_RELAY_E2E=1 to run");
        return Ok(());
    }

    let endpoints = std::env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINTS")
        .map(|value| comma_separated_endpoints(&value))
        .unwrap_or_else(|_| {
            comma_separated_endpoints(
                "wss://nos.lol,wss://relay.damus.io,wss://discrypt-degraded-relay.invalid",
            )
        });
    if endpoints.len() < 2 {
        return Err(TransportError::SignalingAdapter(
            "public Nostr multi-relay soak requires at least two relay endpoints".to_owned(),
        ));
    }
    assert!(
        endpoints.iter().any(|endpoint| endpoint.contains("invalid"))
            || std::env::var("DISCRYPT_PUBLIC_NOSTR_ALLOW_ALL_HEALTHY").as_deref() == Ok("1"),
        "include one intentionally degraded relay endpoint, or set DISCRYPT_PUBLIC_NOSTR_ALLOW_ALL_HEALTHY=1 for a pure relay-set soak"
    );

    let profile = public_nostr_profile(
        endpoints,
        "public-nostr-multi-relay-soak",
        "public multi-relay set with degraded fallback; opaque envelopes only",
    )?;
    run_public_nostr_two_peer_roundtrip(
        profile,
        "public-nostr-multi-relay-soak",
        "hashed topic and opaque payloads across fallback relay set",
    )
    .await
}

#[cfg(feature = "ipfs-pubsub-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_ipfs_two_peer_signaling_smoke() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_IPFS_E2E").as_deref() != Ok("1") {
        eprintln!("skipping public IPFS/libp2p E2E; set DISCRYPT_PUBLIC_IPFS_E2E=1 and DISCRYPT_PUBLIC_IPFS_BOOTSTRAP_ENDPOINTS to comma-separated multiaddrs to run");
        return Ok(());
    }

    let endpoints = public_ipfs_direct_topic_peer_endpoints()?;
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

    alice_room.leave().await?;
    bob_room.leave().await?;
    alice_session.close().await?;
    bob_session.close().await?;
    Ok(())
}

#[cfg(feature = "discrypt-quic-rendezvous-adapter")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_quic_two_peer_signaling_smoke() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping public Discrypt rendezvous E2E; set DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_E2E=1 and DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT=https://... to run"
        );
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT").map_err(|_| {
        TransportError::SignalingAdapter(
            "DISCRYPT_PUBLIC_QUIC_RENDEZVOUS_ENDPOINT is required for deployed Discrypt rendezvous E2E"
                .to_owned(),
        )
    })?;
    let profile = public_discrypt_rendezvous_profile(endpoint)?;
    let scope_secret = random_bytes::<32>();
    let scope = ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(
            ConnectivityScopeLevel::Dm,
            &scope_secret,
            "public-discrypt-rendezvous-e2e",
        ),
    )?;

    let probe = probe_provider_adapter_roundtrip(
        profile,
        scope,
        &random_bytes::<32>(),
        &random_bytes::<16>(),
    )
    .await?;
    assert!(probe.presence_roundtrip);
    assert!(probe.signal_roundtrip);
    Ok(())
}
