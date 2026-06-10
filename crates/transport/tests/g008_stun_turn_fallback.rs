use discrypt_transport::{
    plan_signaling_adapter_fallback, AdapterFallbackBehavior, ConnectivityConfig,
    ConnectivityPlanner, Endpoint, FallbackLeg, IceServerConfig, ReconnectBackoffPolicy,
    SignalingAdapterKind, SimulatedNat, TransportError, TransportSession, TransportSessionError,
    TransportSessionEvent, TransportSessionState, TurnServerConfig, WebRtcIceTransportPolicy,
    WebRtcNegotiationConfig, WebRtcNegotiator,
};

#[test]
fn deterministic_direct_stun_turn_and_no_turn_fail_closed_matrix() -> Result<(), TransportError> {
    let config = ConnectivityConfig::default();
    let turn_config = ConnectivityConfig {
        overrides: discrypt_transport::EndpointOverrides::new(
            None,
            Some(Endpoint::new("turns:g008-relay.example:5349")),
        ),
        ..ConnectivityConfig::default()
    };

    let direct = ConnectivityPlanner::plan(&config, SimulatedNat::direct())?;
    assert_eq!(direct.selected, FallbackLeg::Stun);
    assert_eq!(direct.attempts.len(), 1);
    assert!(direct.attempts[0].succeeded);
    assert!(direct.route_report().honest_and_ordered());

    let turn = ConnectivityPlanner::plan(&turn_config, SimulatedNat::turn_only())?;
    assert_eq!(turn.selected, FallbackLeg::Turn);
    assert_eq!(turn.attempts.len(), 2);
    assert!(turn.ordered_direct_turn());
    assert!(turn.relay_legs_ciphertext_only());
    assert!(turn.route_report().honest_and_ordered());

    let no_turn_path = SimulatedNat {
        stun_available: false,
        overlay_available: false,
        turn_available: false,
    };
    assert_eq!(
        ConnectivityPlanner::plan(&config, no_turn_path),
        Err(TransportError::NoViablePath)
    );

    Ok(())
}

#[test]
fn adapter_outage_fallback_is_ordered_deduplicated_and_reports_single_selection() {
    let plan = plan_signaling_adapter_fallback(
        &[
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Mqtt,
            SignalingAdapterKind::Nostr,
            SignalingAdapterKind::IpfsPubsub,
            SignalingAdapterKind::DiscryptQuicRendezvous,
        ],
        AdapterFallbackBehavior::TryAll,
        None,
    );

    assert_eq!(plan.behavior, AdapterFallbackBehavior::TryAll);
    assert_eq!(
        plan.attempts.len(),
        4,
        "duplicate adapter kinds must collapse"
    );
    assert!(plan.attempts.iter().all(|attempt| attempt.attempted));
    assert!(plan.attempts.iter().all(|attempt| {
        attempt.selected == attempt.readiness.selectable() && Some(attempt.kind) == plan.selected
            || !attempt.selected
    }));
    assert!(
        plan.attempts
            .iter()
            .filter(|attempt| attempt.selected)
            .count()
            <= 1
    );
}

#[test]
fn reconnect_backoff_and_duplicate_session_starts_are_guarded() -> Result<(), TransportSessionError>
{
    let policy = ReconnectBackoffPolicy::new(125, 1_000, 2, 2)?;
    assert_eq!(policy.delay_for_attempt(1), 125);
    assert_eq!(policy.delay_for_attempt(2), 250);
    assert_eq!(policy.delay_for_attempt(9), 1_000);

    let mut session = TransportSession::new();
    session.begin_signaling()?;
    assert_eq!(
        session.begin_signaling(),
        Err(TransportSessionError::InvalidTransition {
            from: TransportSessionState::Signaling,
            event: TransportSessionEvent::StartSignaling,
        })
    );

    session.begin_ice_gathering()?;
    assert_eq!(
        session.begin_ice_gathering(),
        Err(TransportSessionError::InvalidTransition {
            from: TransportSessionState::IceGathering,
            event: TransportSessionEvent::StartIceGathering,
        })
    );

    session.begin_checking()?;
    session.select_direct(Endpoint::new("stun:g008-direct.example:3478"))?;
    assert_eq!(
        session.begin_signaling(),
        Err(TransportSessionError::InvalidTransition {
            from: TransportSessionState::Direct,
            event: TransportSessionEvent::StartSignaling,
        })
    );

    session.mark_disconnected("candidate pair failed")?;
    let reconnect = session.schedule_reconnect(policy)?;
    assert_eq!(reconnect.attempt, 1);
    assert_eq!(reconnect.delay_ms, 125);
    assert_eq!(
        session.schedule_reconnect(policy),
        Err(TransportSessionError::InvalidTransition {
            from: TransportSessionState::Reconnecting,
            event: TransportSessionEvent::StartReconnecting,
        })
    );

    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_only_without_turn_config_fails_closed() -> Result<(), TransportError> {
    let mut config = WebRtcNegotiationConfig::new(IceServerConfig::new(
        vec![Endpoint::new("stun:127.0.0.1:3478")],
        vec![],
    )?);
    config.ice_transport_policy = WebRtcIceTransportPolicy::RelayOnly;

    let error = match WebRtcNegotiator::new(config).await {
        Err(error) => error,
        Ok(negotiator) => {
            negotiator.close().await?;
            return Err(TransportError::InvalidIcePolicy(
                "relay-only WebRTC unexpectedly started without TURN".to_owned(),
            ));
        }
    };
    assert!(format!("{error}").contains("relay-only WebRTC policy requires"));
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn credentialed_turn_config_is_env_gated_and_redacted() -> Result<(), TransportError> {
    if std::env::var("DISCRYPT_PUBLIC_TURN_E2E").as_deref() != Ok("1") {
        eprintln!(
            "skipping configured TURN credential smoke; set DISCRYPT_PUBLIC_TURN_E2E=1 \
             with DISCRYPT_PUBLIC_TURN_ENDPOINT/USERNAME/CREDENTIAL to run"
        );
        return Ok(());
    }

    let endpoint = std::env::var("DISCRYPT_PUBLIC_TURN_ENDPOINT").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "DISCRYPT_PUBLIC_TURN_ENDPOINT is required when DISCRYPT_PUBLIC_TURN_E2E=1".to_owned(),
        )
    })?;
    let username = std::env::var("DISCRYPT_PUBLIC_TURN_USERNAME").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "DISCRYPT_PUBLIC_TURN_USERNAME is required when DISCRYPT_PUBLIC_TURN_E2E=1".to_owned(),
        )
    })?;
    let credential = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL").map_err(|_| {
        TransportError::InvalidIcePolicy(
            "DISCRYPT_PUBLIC_TURN_CREDENTIAL is required when DISCRYPT_PUBLIC_TURN_E2E=1"
                .to_owned(),
        )
    })?;
    let expires_at = std::env::var("DISCRYPT_PUBLIC_TURN_CREDENTIAL_EXPIRES_AT").ok();

    let turn = TurnServerConfig::new(
        Endpoint::new(endpoint),
        Some(username),
        Some(credential.clone()),
        expires_at,
    );
    assert!(!format!("{turn:?}").contains(&credential));

    let ice = IceServerConfig::new(Vec::new(), vec![turn])?;
    let mut config = WebRtcNegotiationConfig::new(ice);
    config.udp_addrs = vec!["127.0.0.1:0".to_owned()];
    config.ice_transport_policy = WebRtcIceTransportPolicy::RelayOnly;

    let negotiator = WebRtcNegotiator::new(config).await?;
    let metrics = negotiator.direct_path_metrics().await;
    assert_eq!(metrics.configured_turn_servers, 1);
    assert!(!format!("{metrics:?}").contains(&credential));
    negotiator.close().await?;
    Ok(())
}
