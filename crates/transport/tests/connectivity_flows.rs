use discrypt_transport::{
    ConnectivityConfig, ConnectivityPlanner, Endpoint, FallbackLeg, LocalProcessSocketAdapter,
    SimulatedNat, TransportError,
};

#[test]
fn valid_direct_and_configured_turn_flows_select_expected_leg() -> Result<(), TransportError> {
    let config = ConnectivityConfig::default();
    let turn_config = ConnectivityConfig {
        default_turn: Endpoint::new("turns:transport-session-test.invalid:5349"),
        ..ConnectivityConfig::default()
    };
    let cases = [
        (&config, SimulatedNat::direct(), FallbackLeg::Stun, 1),
        (
            &turn_config,
            SimulatedNat::turn_only(),
            FallbackLeg::Turn,
            2,
        ),
    ];

    for (config, nat, expected_leg, expected_attempts) in cases {
        let plan = ConnectivityPlanner::plan(config, nat)?;

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
        assert!(plan.ordered_direct_turn());
        assert!(plan.relay_legs_ciphertext_only());
        assert!(plan.route_report().honest_and_ordered());
    }

    Ok(())
}

#[test]
fn turn_flow_uses_effective_turn_endpoint_without_parsing_ice_config() -> Result<(), TransportError>
{
    let config = ConnectivityConfig {
        default_turn: Endpoint::new("turns:transport-session-test.invalid:5349"),
        ..ConnectivityConfig::default()
    };

    let plan = ConnectivityPlanner::plan(&config, SimulatedNat::turn_only())?;

    assert_eq!(plan.selected, FallbackLeg::Turn);
    assert_eq!(
        plan.endpoint,
        Endpoint::new("turns:transport-session-test.invalid:5349")
    );
    assert_eq!(plan.attempts[1].endpoint, plan.endpoint);
    assert!(plan.attempts[1].ciphertext_only);
    assert!(!plan.attempts[1].carries_content);
    Ok(())
}

#[test]
fn disconnected_local_socket_flow_can_reconnect_with_ciphertext_only_delivery(
) -> Result<(), TransportError> {
    let adapter = LocalProcessSocketAdapter::new(
        ConnectivityConfig::default(),
        SimulatedNat::direct(),
        b"cleartext session payload".to_vec(),
    );

    let empty_error = match adapter.run_conformance(b"") {
        Err(error) => error,
        Ok(report) => {
            return Err(TransportError::Unavailable(format!(
                "unexpected report: {report:?}"
            )))
        }
    };
    assert_eq!(
        empty_error,
        TransportError::Unavailable("empty ciphertext".to_owned())
    );

    let first_reconnect = adapter.run_conformance(b"ciphertext after reconnect one")?;
    assert!(first_reconnect.ready());
    assert_eq!(first_reconnect.route_report.selected, FallbackLeg::Stun);

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
        ConnectivityConfig {
            default_turn: Endpoint::new("turns:transport-session-test.invalid:5349"),
            ..ConnectivityConfig::default()
        },
        SimulatedNat::turn_only(),
        b"forbidden plaintext".to_vec(),
    );

    let plaintext_error = match adapter.run_conformance(b"prefix forbidden plaintext suffix") {
        Err(error) => error,
        Ok(report) => {
            return Err(TransportError::Unavailable(format!(
                "unexpected report: {report:?}"
            )))
        }
    };
    assert_eq!(plaintext_error, TransportError::PlaintextLeak);

    let recovered = adapter.run_conformance(b"opaque sframe bytes after plaintext rejection")?;
    assert!(recovered.ready());
    assert_eq!(recovered.route_report.selected, FallbackLeg::Turn);
    Ok(())
}

#[test]
fn turn_required_without_configured_relay_fails_closed() {
    let config = ConnectivityConfig::default();

    assert_eq!(
        ConnectivityPlanner::plan(&config, SimulatedNat::turn_only()),
        Err(TransportError::NoViablePath)
    );
}

#[test]
fn failure_flow_reports_no_viable_path_when_direct_and_turn_fail() {
    let config = ConnectivityConfig {
        default_turn: Endpoint::new("turns:transport-session-test.invalid:5349"),
        ..ConnectivityConfig::default()
    };
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
