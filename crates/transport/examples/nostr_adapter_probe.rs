//! Diagnostic Nostr ADAPTER probe: joins two rooms through the transport's
//! own NostrProviderAdapter (exercising join/notifications/record/take exactly
//! like the app), publishes a control payload from room A, and expects room B
//! to take it.
//!
//! Usage: nostr_adapter_probe [relay] — defaults to wss://relay.damus.io

#[cfg(feature = "nostr-adapter")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use discrypt_transport::{
        AdapterTrustLabel, ConnectivityScopeLevel, Endpoint, OpaqueSignalingPayload,
        ProviderMetadataPosture, SignalingAdapterCapabilities, SignalingAdapterKind,
        SignalingAdapterProfile, SignalingEndpointSecurity, SignalingPeerId,
        SignalingProviderEndpoint,
    };

    let relay = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "wss://relay.damus.io".to_owned());
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async move {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let profile = SignalingAdapterProfile {
            profile_id: "nostr-adapter-probe".to_owned(),
            kind: SignalingAdapterKind::Nostr,
            endpoints: vec![SignalingProviderEndpoint::new(
                Endpoint::new(&relay),
                SignalingEndpointSecurity::ProductionTls,
            )],
            metadata_posture: ProviderMetadataPosture::HashedTopic,
            capabilities: SignalingAdapterCapabilities::production_required(),
            trust_label: AdapterTrustLabel::new(
                SignalingAdapterKind::Nostr.canonical_name(),
                "redacted boundary",
            )?,
        };
        let scope = discrypt_transport::ConversationScope::new(
            ConnectivityScopeLevel::Group,
            "probe-scope-id-commitment-0123456789abcdef".to_owned(),
        )
        .map_err(|err| format!("scope: {err:?}"))?;
        let bootstrap_secret = [42_u8; 32];
        let random_entropy = [7_u8; 16];
        let peer_a = SignalingPeerId::new("probe-peer-a")?;
        let peer_b = SignalingPeerId::new("probe-peer-b")?;

        let room_a = discrypt_transport::join_provider_control_lane_room(
            profile.clone(),
            scope.clone(),
            &bootstrap_secret,
            &random_entropy,
            peer_a.clone(),
        )
        .await
        .map_err(|err| format!("room A join failed: {err:?}"))?;
        println!("room A joined");
        let room_b = discrypt_transport::join_provider_control_lane_room(
            profile,
            scope,
            &bootstrap_secret,
            &random_entropy,
            peer_b.clone(),
        )
        .await
        .map_err(|err| format!("room B join failed: {err:?}"))?;
        println!("room B joined");

        let payload = OpaqueSignalingPayload::new(b"probe-control-payload".to_vec())?;
        room_a
            .broadcast_control(payload)
            .await
            .map_err(|err| format!("A publish failed: {err:?}"))?;
        println!("A published a control payload");

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                println!("probe: B DID NOT TAKE A's payload within 30s");
                break;
            }
            let taken = room_b.take_control_payloads().await?;
            if !taken.is_empty() {
                println!(
                    "B TOOK {} control payload(s) from A — adapter round-trip works",
                    taken.len()
                );
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        }
        Ok(())
    })
}

#[cfg(not(feature = "nostr-adapter"))]
fn main() {
    println!("nostr_adapter_probe requires building with --features nostr-adapter");
}
