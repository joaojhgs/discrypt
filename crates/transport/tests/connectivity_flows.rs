use discrypt_transport::{
    ConnectivityConfig, ConnectivityPlanner, Endpoint, FallbackLeg, LocalProcessSocketAdapter,
    SimulatedNat, TransportError,
};

#[test]
fn valid_direct_overlay_and_turn_flows_select_expected_leg() -> Result<(), TransportError> {
    let config = ConnectivityConfig::default();
    let cases = [
        (SimulatedNat::direct(), FallbackLeg::Stun, 1),
        (SimulatedNat::overlay_only(), FallbackLeg::RelayOverlay, 2),
        (SimulatedNat::turn_only(), FallbackLeg::Turn, 3),
    ];

    for (nat, expected_leg, expected_attempts) in cases {
        let plan = ConnectivityPlanner::plan(&config, nat)?;

        assert_eq!(plan.selected, expected_leg);
        assert_eq!(plan.attempts.len(), expected_attempts);
        assert_eq!(
            plan.attempts.last().map(|attempt| attempt.leg),
            Some(expected_leg)
        );
        assert!(plan
            .attempts
            .last()
            .is_some_and(|attempt| attempt.succeeded));
        assert!(plan.ordered_stun_overlay_turn());
        assert!(plan.relay_legs_ciphertext_only());
        assert!(plan.route_report().honest_and_ordered());
    }

    Ok(())
}

#[test]
fn turn_flow_uses_effective_turn_endpoint_without_parsing_ice_config() -> Result<(), TransportError>
{
    let mut config = ConnectivityConfig::default();
    config.default_turn = Endpoint::new("turns:transport-session-test.invalid:5349");

    let plan = ConnectivityPlanner::plan(&config, SimulatedNat::turn_only())?;

    assert_eq!(plan.selected, FallbackLeg::Turn);
    assert_eq!(
        plan.endpoint,
        Endpoint::new("turns:transport-session-test.invalid:5349")
    );
    assert_eq!(plan.attempts[2].endpoint, plan.endpoint);
    assert!(plan.attempts[2].ciphertext_only);
    assert!(!plan.attempts[2].carries_content);
    Ok(())
}

#[test]
fn disconnected_local_socket_flow_can_reconnect_with_ciphertext_only_delivery(
) -> Result<(), TransportError> {
    let adapter = LocalProcessSocketAdapter::new(
        ConnectivityConfig::default(),
        SimulatedNat::overlay_only(),
        b"cleartext session payload".to_vec(),
    );

    assert_eq!(
        adapter.run_conformance(b"").unwrap_err(),
        TransportError::Unavailable("empty ciphertext".to_owned())
    );

    let first_reconnect = adapter.run_conformance(b"ciphertext after reconnect one")?;
    assert!(first_reconnect.ready());
    assert_eq!(
        first_reconnect.route_report.selected,
        FallbackLeg::RelayOverlay
    );

    let second_reconnect = adapter.run_conformance(b"ciphertext after reconnect two")?;
    assert!(second_reconnect.ready());
    assert_eq!(
        second_reconnect.delivered_len,
        b"ciphertext after reconnect two".len()
    );
    Ok(())
}

#[test]
fn plaintext_failure_does_not_prevent_later_reconnect() -> Result<(), TransportError> {
    let adapter = LocalProcessSocketAdapter::new(
        ConnectivityConfig::default(),
        SimulatedNat::turn_only(),
        b"forbidden plaintext".to_vec(),
    );

    assert_eq!(
        adapter
            .run_conformance(b"prefix forbidden plaintext suffix")
            .unwrap_err(),
        TransportError::PlaintextLeak
    );

    let recovered = adapter.run_conformance(b"opaque sframe bytes after plaintext rejection")?;
    assert!(recovered.ready());
    assert_eq!(recovered.route_report.selected, FallbackLeg::Turn);
    Ok(())
}

#[test]
fn failure_flow_reports_no_viable_path_when_direct_overlay_and_turn_all_fail() {
    let config = ConnectivityConfig::default();
    let unreachable_nat = SimulatedNat {
        stun_available: false,
        overlay_available: false,
        turn_available: false,
    };

    assert_eq!(
        ConnectivityPlanner::plan(&config, unreachable_nat),
        Err(TransportError::NoViablePath)
    );
}
