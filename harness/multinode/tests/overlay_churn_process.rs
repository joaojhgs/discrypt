use std::process::Command;

use discrypt_relay_overlay::{OverlayManager, RelayRuntimeObservation};

fn observation(peer_id: &str, latency_ms: u32) -> RelayRuntimeObservation {
    RelayRuntimeObservation {
        peer_id: peer_id.to_owned(),
        latency_ms,
        successful_probes: 10,
        failed_probes: 0,
        battery_cost_bps: 0,
        contributed_bytes: 10_000,
        consumed_bytes: 0,
    }
}

#[test]
fn sixteen_node_process_harness_churn_loss_meets_overlay_gates(
) -> Result<(), Box<dyn std::error::Error>> {
    let bin = env!("CARGO_BIN_EXE_discrypt-multinode-harness");
    let mut reports = Vec::new();
    for index in 0..16 {
        let output = Command::new(bin)
            .args(["--overlay-node-report", &index.to_string()])
            .output()?;
        assert!(
            output.status.success(),
            "node {index} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout)?;
        assert!(stdout.contains(&format!("node_index={index}")));
        assert!(stdout.contains("relay_visible_ciphertext_only=true"));
        assert!(stdout.contains("tamper_rejected=true"));
        assert!(stdout.contains("replay_rejected=true"));
        reports.push(stdout);
    }
    assert_eq!(reports.len(), 16);

    let mut manager = OverlayManager::default();
    for index in 0..16 {
        manager.upsert_observation(observation(
            &format!("node-{index}"),
            if index == 2 { 30 } else { 10 },
        ))?;
    }
    manager.connect_peers("node-0", "node-1")?;
    manager.connect_peers("node-1", "node-15")?;
    manager.connect_peers("node-0", "node-2")?;
    manager.connect_peers("node-2", "node-15")?;
    for index in 3..=8 {
        manager.connect_peers(&format!("node-{index}"), "node-2")?;
    }
    for index in 9..=13 {
        manager.connect_peers(&format!("node-{index}"), "node-0")?;
    }
    manager.connect_peers("node-14", "node-15")?;

    let previous = manager.route("node-0", "node-15")?.route;
    assert!(previous.within_hop_limit());
    assert_eq!(previous.path, ["node-0", "node-1", "node-15"]);

    let failover = manager.mark_failed_media_and_reroute(previous, "node-1", 2_750, 180)?;
    assert!(failover.decision.replacement.within_hop_limit());
    assert!(failover.decision.converged_within_phase2_gate());
    assert_eq!(
        failover.decision.replacement.path,
        ["node-0", "node-2", "node-15"]
    );
    let concealment = failover
        .media_concealment
        .as_ref()
        .ok_or_else(|| std::io::Error::other("media concealment report missing"))?;
    assert!(concealment.target_met);
    assert!(concealment.observed_gap_ms <= 200);
    assert!(!failover.decision.replacement.contains_peer("node-1"));
    Ok(())
}
