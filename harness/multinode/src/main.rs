use discrypt_core::summarize;
use discrypt_mls_core::GroupState;
use discrypt_multinode_harness::{
    connectivity_signaling_push_smoke, governance_admission_smoke, media_security_smoke,
    overlay_node_process_report, phase_c_device_rotation_smoke, relay_overlay_smoke,
    retention_shred_smoke, storage_persistence_smoke, text_history_delivery_smoke,
    ux_e2e_hardening_smoke,
};

fn main() {
    let args = std::env::args().collect::<Vec<_>>();
    if args
        .get(1)
        .is_some_and(|arg| arg == "--overlay-node-report")
    {
        let node_index = args
            .get(2)
            .and_then(|raw| raw.parse::<usize>().ok())
            .unwrap_or_default();
        match overlay_node_process_report(node_index) {
            Ok(report) => {
                println!("{}", report.to_line());
                return;
            }
            Err(error) => {
                eprintln!("discrypt overlay node report failed: {error}");
                std::process::exit(1);
            }
        }
    }

    let group = GroupState::new("phase0-smoke");
    let summary = summarize(&group);
    match (
        media_security_smoke(),
        relay_overlay_smoke(),
        text_history_delivery_smoke(),
        retention_shred_smoke(),
        storage_persistence_smoke(),
        governance_admission_smoke(),
        connectivity_signaling_push_smoke(),
        phase_c_device_rotation_smoke(),
        ux_e2e_hardening_smoke(),
    ) {
        (
            Ok(media),
            Ok(overlay),
            Ok(text),
            Ok(retention),
            Ok(storage),
            Ok(governance),
            Ok(connectivity),
            Ok(phase_c),
            Ok(ux),
        ) => {
            if !(media.ready()
                && overlay.ready()
                && text.ready()
                && retention.ready()
                && storage.ready()
                && governance.ready()
                && connectivity.ready()
                && phase_c.ready()
                && ux.ready())
            {
                eprintln!("discrypt multinode harness readiness gate failed");
                std::process::exit(1);
            }
            println!(
                "discrypt multinode harness: room={} epoch={} members={} media_passive={} media_replay={} media_tamper={} overlay_hops={} overlay_failover={} overlay_media_gap={} overlay_redelivery={} overlay_store_forward_plaintext={} overlay_store_forward_ttl={} overlay_store_forward_fanout={} overlay_ciphertext_only={} overlay_tamper={} text_e2e_roundtrip={} text_direct_path={} text_overlay_path={} text_turn_path={} text_offline_store_forward={} text_retention_locks_old={} text_pcap_no_plaintext={} text_no_plaintext={} text_author_logs={} text_cache_bounded={} text_gossip16={} text_ordered_delivery={} text_welcome_catchup={} text_fork_detected={} text_repair_converged={} text_no_mls_replay={} retention_default_lock={} retention_transition={} shred_cross_device={} live_key_oracle={} secure_delete={} recovery_no_content_keys={} storage_fresh_install={} storage_restart={} storage_no_plaintext={} storage_corrupt_rejected={} storage_secure_delete={} gov_ordered={} gov_invalid_rejected={} gov_removed_admin={} admission_invites={} admission_password_welcome={} recovery_trust={} abuse_controls={} signaling_zero_linkage={} connectivity_fallback={} connectivity_overrides={} android_wake_content_free={} metadata_matrix={} pcap_no_content={} relays_ciphertext_only={} socket_local_process={} route_reporting_honest={} phase_c_retired={} phase_c_rekeyed={} phase_c_old_send_blocked={} phase_c_replacement_send={} phase_c_stale_blocked={} phase_c_transparency={} phase_c_command_devices={} ux_commands={} ux_discord={} ux_verify_devices={} ux_invite_retention_delete={} ux_connectivity_copy={} ux_all_phases={}",
            summary.room_id,
            summary.epoch,
            summary.members,
            media.passive_relay_cannot_read,
            media.replay_rejected,
            media.tamper_rejected,
            overlay.hop_limit_respected,
            overlay.failover_recovered,
            overlay.media_gap_concealed,
            overlay.redelivery_replay_rejected,
            overlay.store_forward_plaintext_rejected,
            overlay.store_forward_ttl_enforced,
            overlay.store_forward_fanout_bounded,
            overlay.ciphertext_only_media,
            overlay.tamper_rejected,
            text.text_e2e_roundtrip,
            text.direct_path_text_exchanged,
            text.overlay_path_text_exchanged,
            text.turn_path_text_exchanged,
            text.offline_store_forward_within_ttl,
            text.retention_locks_old_store_forward,
            text.text_pcap_no_plaintext,
            text.no_plaintext_in_text_surfaces,
            text.author_logs_merged,
            text.recipient_cache_bounded,
            text.gossip_converged_16,
            text.ordered_commit_delivery,
            text.welcome_catchup_live,
            text.fork_detected_not_silent,
            text.repair_converged_equal_tags,
            text.divergent_mls_not_replayed,
            retention.default_window_locks_old_messages,
            retention.shorten_retro_lengthen_future,
            retention.cross_device_shred_sync,
            retention.live_key_membership_rate_limit_decoy,
            retention.secure_delete_negative,
            retention.recovery_cannot_resurrect_content_keys,
            storage.fresh_install_starts_empty,
            storage.restart_loads_encrypted_state,
            storage.no_plaintext_in_db_wal_or_temp,
            storage.corrupted_store_rejected,
            storage.secure_delete_requires_db_wal_and_keychain,
            governance.governance_ordered_signed,
            governance.governance_rejects_invalid_authority,
            governance.removed_admin_cannot_win,
            governance.invite_controls_enforced,
            governance.password_and_welcome_gate,
            governance.recovery_trust_model,
            governance.abuse_controls_enforced,
            connectivity.signaling_zero_linkage_at_rest,
            connectivity.fallback_chain_covered,
            connectivity.owner_overrides_used,
            connectivity.android_wake_content_free,
            connectivity.metadata_matrix_validated,
            connectivity.pcap_no_central_content,
            connectivity.relays_ciphertext_only,
            connectivity.socket_local_process_conformant,
            connectivity.route_reporting_honest,
            phase_c.compromised_device_retired,
            phase_c.group_rekeyed_after_rotation,
            phase_c.old_device_send_blocked,
            phase_c.replacement_device_can_send,
            phase_c.stale_epoch_send_blocked,
            phase_c.transparency_notices_include_rotation,
            phase_c.command_surface_reports_device_metadata,
            ux.command_surface_ready,
            ux.discord_style_model_ready,
            ux.verification_and_devices_ready,
            ux.invite_retention_deletion_ready,
            ux.connectivity_copy_ready,
                ux.all_phase_smokes_ready
            );
        }
        (Err(error), _, _, _, _, _, _, _, _) => {
            eprintln!("discrypt multinode harness media smoke failed: {error}");
            std::process::exit(1);
        }
        (_, Err(error), _, _, _, _, _, _, _) => {
            eprintln!("discrypt multinode harness relay overlay smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, Err(error), _, _, _, _, _, _) => {
            eprintln!("discrypt multinode harness text history smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, Err(error), _, _, _, _, _) => {
            eprintln!("discrypt multinode harness retention/shred smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, Err(error), _, _, _, _) => {
            eprintln!("discrypt multinode harness storage persistence smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, _, Err(error), _, _, _) => {
            eprintln!("discrypt multinode harness governance/admission smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, _, _, Err(error), _, _) => {
            eprintln!(
                "discrypt multinode harness connectivity/signaling/push smoke failed: {error}"
            );
            std::process::exit(1);
        }
        (_, _, _, _, _, _, _, Err(error), _) => {
            eprintln!("discrypt multinode harness Phase-C device rotation smoke failed: {error}");
            std::process::exit(1);
        }
        (_, _, _, _, _, _, _, _, Err(error)) => {
            eprintln!("discrypt multinode harness UX/E2E hardening smoke failed: {error}");
            std::process::exit(1);
        }
    }
}
