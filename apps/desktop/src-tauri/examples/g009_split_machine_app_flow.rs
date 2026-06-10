use chrono::Utc;
use discrypt_core::ChannelKind;
use discrypt_desktop::{
    app_state, attach_text_control_transport_runtime, create_group, create_invite, create_user,
    join_group, join_voice, promote_group_member_to_staff, pump_text_control_transport_once,
    revoke_group_member_access, send_message, set_active_group, start_text_session,
    AttachTextControlTransportRuntimeRequest, CreateGroupRequest, CreateInviteRequest,
    CreateUserRequest, GroupAdmissionModeView, GroupRoleView, JoinGroupRequest, JoinVoiceRequest,
    ListPendingTextControlFramesRequest, MessageTargetView, PromoteGroupMemberRequest,
    RevokeGroupMemberAccessRequest, SendMessageRequest, SetActiveGroupRequest,
    StartTextSessionRequest,
};
use serde::Serialize;
use serde_json::json;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
struct Args {
    role: String,
    artifact: PathBuf,
    invite: Option<String>,
    adapter: String,
    endpoint: String,
    group_name: String,
    timeout_secs: u64,
}

#[derive(Clone, Debug, Serialize)]
struct PumpEvidence {
    label: String,
    frames_sent: usize,
    response_frames_received: usize,
    receipts_applied: usize,
    failures: Vec<String>,
    runtime_open: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = parse_args()?;
    if let Some(parent) = args.artifact.parent() {
        fs::create_dir_all(parent)?;
    }
    let started_at = Utc::now().to_rfc3339();
    let evidence = match args.role.as_str() {
        "prepare-owner" => prepare_owner(&args)?,
        "owner" => run_owner(&args)?,
        "joiner" => run_joiner(&args)?,
        other => return Err(format!("unsupported --role {other}").into()),
    };
    let doc = json!({
        "schema_version": "discrypt.g009.split_machine_app_flow.v1",
        "status": "passed",
        "role": args.role,
        "adapter": args.adapter,
        "endpoint": args.endpoint,
        "started_at": started_at,
        "completed_at": Utc::now().to_rfc3339(),
        "state_path": env::var("DISCRYPT_APP_STATE_PATH").unwrap_or_else(|_| "default".to_owned()),
        "evidence": evidence,
    });
    fs::write(&args.artifact, serde_json::to_vec_pretty(&doc)?)?;
    println!("g009 app-flow artifact: {}", args.artifact.display());
    Ok(())
}

fn prepare_owner(args: &Args) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let created_user = create_user(CreateUserRequest {
        display_name: "G009 Owner".to_owned(),
        device_name: Some("local owner machine".to_owned()),
    });
    ensure_ok(&created_user.last_command_error, "create owner user")?;
    let group = create_group(CreateGroupRequest {
        name: args.group_name.clone(),
        retention: "7 days".to_owned(),
        admission_mode: Some(GroupAdmissionModeView::AutomaticWhenAuthorizedOnline),
        adapter_kind: Some(args.adapter.clone()),
        signaling_endpoint: Some(args.endpoint.clone()),
        ice_stun_servers: Some(vec!["stun:stun.l.google.com:19302".to_owned()]),
        ice_turn_servers: Some(vec![]),
    });
    ensure_ok(&group.last_command_error, "create owner group")?;
    let group = group
        .groups
        .iter()
        .find(|group| group.name == args.group_name)
        .ok_or("created group missing")?;
    let group_id = group.group_id.clone();
    set_active_group(SetActiveGroupRequest {
        group_id: group_id.clone(),
    });
    let channel_id = group
        .channels
        .iter()
        .find(|channel| matches!(channel.kind, ChannelKind::Text))
        .or_else(|| group.channels.first())
        .ok_or("created group has no text channel")?
        .channel_id
        .clone();
    let voice_channel_id = group
        .channels
        .iter()
        .find(|channel| matches!(channel.kind, ChannelKind::Voice))
        .map(|channel| channel.channel_id.clone())
        .unwrap_or_else(|| "voice-lobby".to_owned());
    let invite_state = create_invite(CreateInviteRequest {
        group_id: Some(group_id.clone()),
        expires: "1 day".to_owned(),
        max_use: "5".to_owned(),
        password_gate: None,
    });
    ensure_ok(&invite_state.last_command_error, "create invite")?;
    let invite = invite_state
        .invites
        .last()
        .ok_or("invite missing")?
        .code
        .clone();
    Ok(json!({
        "owner_account_created": true,
        "group_name": args.group_name,
        "group_id": group_id,
        "channel_id": channel_id,
        "voice_channel_id": voice_channel_id,
        "invite": invite,
        "invite_contains_gid": invite.contains("gid="),
        "invite_omits_unsigned_gname": !invite.contains("gname="),
    }))
}

fn run_joiner(args: &Args) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let invite = args.invite.clone().ok_or("joiner requires --invite")?;
    let created_user = create_user(CreateUserRequest {
        display_name: "G009 Joiner".to_owned(),
        device_name: Some("ssh joiner machine".to_owned()),
    });
    ensure_ok(&created_user.last_command_error, "create joiner user")?;
    let joined = join_group(JoinGroupRequest {
        invite_code: invite,
        group_name: None,
    });
    ensure_ok(&joined.last_command_error, "join group from invite")?;
    let group = joined
        .groups
        .first()
        .ok_or("joiner group missing after invite")?;
    let group_id = group.group_id.clone();
    let channel_id = group
        .channels
        .first()
        .ok_or("joiner text channel missing")?
        .channel_id
        .clone();
    let voice_channel_id = group
        .channels
        .iter()
        .find(|channel| matches!(channel.kind, ChannelKind::Voice))
        .map(|channel| channel.channel_id.clone())
        .unwrap_or_else(|| "voice-lobby".to_owned());
    join_voice(JoinVoiceRequest {
        group_id: group_id.clone(),
        channel_id: voice_channel_id.clone(),
        microphone_permission: "granted".to_owned(),
        input_device_id: Some("g009-virtual-mic".to_owned()),
        input_device_label: Some("G009 virtual microphone".to_owned()),
        output_device_id: Some("g009-virtual-speaker".to_owned()),
        output_device_label: Some("G009 virtual speaker".to_owned()),
    });
    let runtime_attached_direct_path = start_and_attach_runtime_derived_or_relay()?;
    let admission = pump_until("joiner-admission", args.timeout_secs, |report| {
        report.response_frames_received > 0 && report.failures.is_empty()
    })?;
    let target = MessageTargetView {
        kind: "channel".to_owned(),
        dm_id: None,
        group_id: Some(group_id.clone()),
        channel_id: Some(channel_id.clone()),
    };
    wait_until(args.timeout_secs, "joiner OpenMLS send readiness", || {
        let sent = send_message(SendMessageRequest {
            target: target.clone(),
            body: "g009 joiner to owner protected text".to_owned(),
            transport_proof: false,
            adapter_kind: None,
        });
        sent.last_command_error.is_none()
    })?;
    let joiner_text = pump_until("joiner-to-owner-text", args.timeout_secs, |report| {
        report.receipts_applied > 0 && report.failures.is_empty()
    })?;
    wait_for_message("g009 owner to joiner protected text", args.timeout_secs)?;
    let local_member_id = app_state()
        .profile
        .as_ref()
        .map(|profile| profile.user_id.clone())
        .ok_or("joiner profile missing before governance wait")?;
    let promotion_seen =
        wait_for_state_with_pump(args.timeout_secs, "joiner promoted to staff", || {
            app_state().groups.iter().any(|group| {
                group.group_id == group_id
                    && group.members.iter().any(|member| {
                        member.member_id == local_member_id
                            && format!("{:?}", member.role)
                                .to_lowercase()
                                .contains("staff")
                    })
            })
        })?;
    let revoke_seen = wait_for_state_with_pump(args.timeout_secs, "joiner revoked", || {
        app_state().groups.iter().any(|group| {
            group.group_id == group_id
                && group
                    .members
                    .iter()
                    .any(|member| member.member_id == local_member_id && member.status == "revoked")
        })
    })?;
    let revoked_send = send_message(SendMessageRequest {
        target,
        body: "g009 revoked member should not send".to_owned(),
        transport_proof: false,
        adapter_kind: None,
    });
    Ok(json!({
        "account_created": true,
        "joined_group_id": group_id,
        "joined_group_name": app_state().groups.first().map(|g| g.name.clone()),
        "runtime_attached_direct_path": runtime_attached_direct_path,
        "admission": admission,
        "joiner_text": joiner_text,
        "received_owner_text": true,
        "promotion_seen": promotion_seen,
        "revoke_seen": revoke_seen,
        "voice_joined_with_native_boundary": app_state().voice_session.is_some(),
        "revoked_send_error": revoked_send.last_command_error.map(|error| error.code),
    }))
}

fn run_owner(args: &Args) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let runtime_attached_direct_path = start_and_attach_runtime_derived_or_relay()?;
    let group = app_state()
        .groups
        .first()
        .ok_or("owner group missing")?
        .clone();
    let group_id = group.group_id.clone();
    let channel_id = group
        .channels
        .first()
        .ok_or("owner text channel missing")?
        .channel_id
        .clone();
    let voice_channel_id = group
        .channels
        .iter()
        .find(|channel| matches!(channel.kind, ChannelKind::Voice))
        .map(|channel| channel.channel_id.clone())
        .unwrap_or_else(|| "voice-lobby".to_owned());
    join_voice(JoinVoiceRequest {
        group_id: group_id.clone(),
        channel_id: voice_channel_id,
        microphone_permission: "granted".to_owned(),
        input_device_id: Some("g009-local-virtual-mic".to_owned()),
        input_device_label: Some("G009 local virtual microphone".to_owned()),
        output_device_id: Some("g009-local-virtual-speaker".to_owned()),
        output_device_label: Some("G009 local virtual speaker".to_owned()),
    });
    wait_for_state_with_pump(args.timeout_secs, "owner admitted remote member", || {
        app_state()
            .groups
            .first()
            .map(|g| g.members.len())
            .unwrap_or(0)
            > 1
    })?;
    wait_for_message("g009 joiner to owner protected text", args.timeout_secs)?;
    let target = MessageTargetView {
        kind: "channel".to_owned(),
        dm_id: None,
        group_id: Some(group_id.clone()),
        channel_id: Some(channel_id.clone()),
    };
    let sent = send_message(SendMessageRequest {
        target: target.clone(),
        body: "g009 owner to joiner protected text".to_owned(),
        transport_proof: false,
        adapter_kind: None,
    });
    ensure_ok(&sent.last_command_error, "owner send protected text")?;
    let owner_text = pump_until("owner-to-joiner-text", args.timeout_secs, |report| {
        report.receipts_applied > 0 && report.failures.is_empty()
    })?;
    let joiner_member = app_state()
        .groups
        .first()
        .and_then(|group| {
            group
                .members
                .iter()
                .find(|member| member.role != GroupRoleView::Owner)
                .cloned()
        })
        .ok_or("joiner member missing for governance actions")?;
    let promoted = promote_group_member_to_staff(PromoteGroupMemberRequest {
        group_id: group_id.clone(),
        member_id: joiner_member.member_id.clone(),
    });
    ensure_ok(&promoted.last_command_error, "promote joiner to staff")?;
    let promote_pump = pump_until("promote-staff-frame", args.timeout_secs, |report| {
        report.frames_sent > 0 && report.response_frames_received > 0 && report.failures.is_empty()
    })?;
    // Give the answerer-side app callback a small window to persist the role-change before sending
    // the larger OpenMLS remove commit. This mirrors a UI round-trip where staff state renders
    // before a later kick/revoke action is issued.
    thread::sleep(Duration::from_secs(5));
    let revoked = revoke_group_member_access(RevokeGroupMemberAccessRequest {
        group_id: group_id.clone(),
        member_id: joiner_member.member_id,
        reason: Some("g009 e2e revoke proof".to_owned()),
    });
    ensure_ok(&revoked.last_command_error, "revoke joiner")?;
    let revoke_pump = pump_until("revoke-frame", args.timeout_secs, |report| {
        report.frames_sent > 0 && report.metrics.open
    })?;
    Ok(json!({
        "owner_runtime_attached_direct_path": runtime_attached_direct_path,
        "owner_text": owner_text,
        "received_joiner_text": true,
        "voice_joined_with_native_boundary": app_state().voice_session.is_some(),
        "promote_pump": promote_pump,
        "revoke_pump": revoke_pump,
        "members": app_state().groups.first().map(|g| g.members.clone()),
    }))
}

fn start_and_attach_runtime_derived_or_relay() -> Result<bool, Box<dyn std::error::Error>> {
    let started = start_text_session(StartTextSessionRequest {
        scope_label: Some("g009-split-machine-app-flow".to_owned()),
        data_channel_probe: false,
        adapter_kind: None,
    });
    ensure_ok(&started.last_command_error, "start text session")?;
    let attached =
        attach_text_control_transport_runtime(AttachTextControlTransportRuntimeRequest {
            session_id: None,
            runtime_role: None,
            local_peer_id: None,
            remote_peer_id: None,
            derive_from_state: true,
        });
    if let Some(error) = attached.last_command_error {
        eprintln!(
            "g009 direct text/control runtime attach unavailable; continuing with provider relay fallback: {}: {}",
            error.code, error.message
        );
        return Ok(false);
    }
    match wait_until(90, "runtime attached", || {
        runtime_status() == Some("attached".to_owned())
    }) {
        Ok(()) => Ok(true),
        Err(error) => {
            eprintln!(
                "g009 direct text/control runtime did not become attached; continuing with provider relay fallback: {error}"
            );
            Ok(false)
        }
    }
}

fn pump_until<F>(
    label: &str,
    timeout_secs: u64,
    done: F,
) -> Result<PumpEvidence, Box<dyn std::error::Error>>
where
    F: Fn(&discrypt_desktop::TextControlTransportPumpReportView) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let mut last = PumpEvidence {
        label: label.to_owned(),
        frames_sent: 0,
        response_frames_received: 0,
        receipts_applied: 0,
        failures: Vec::new(),
        runtime_open: false,
    };
    while Instant::now() < deadline {
        let report = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: None,
            limit: Some(16),
            operation_timeout_ms: Some(30_000),
        });
        last = PumpEvidence {
            label: label.to_owned(),
            frames_sent: report.frames_sent,
            response_frames_received: report.response_frames_received,
            receipts_applied: report.receipts_applied,
            failures: report.failures.clone(),
            runtime_open: report.metrics.open,
        };
        if done(&report) {
            return Ok(last);
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!("timed out waiting for pump {label}; last={last:?}").into())
}

fn wait_for_state_with_pump<F>(
    timeout_secs: u64,
    label: &str,
    mut condition: F,
) -> Result<bool, Box<dyn std::error::Error>>
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if condition() {
            return Ok(true);
        }
        let _ = pump_text_control_transport_once(ListPendingTextControlFramesRequest {
            target: None,
            limit: Some(16),
            operation_timeout_ms: Some(5_000),
        });
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!("timed out waiting for {label}").into())
}

fn wait_for_message(body: &str, timeout_secs: u64) -> Result<(), Box<dyn std::error::Error>> {
    wait_for_state_with_pump(timeout_secs, &format!("message {body}"), || {
        app_state()
            .messages
            .iter()
            .any(|message| message.body == body)
    })?;
    Ok(())
}

fn wait_until<F>(
    timeout_secs: u64,
    label: &str,
    mut condition: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        if condition() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(500));
    }
    Err(format!(
        "timed out waiting for {label}; runtime={:?}; last_error={:?}",
        runtime_status(),
        app_state().last_command_error
    )
    .into())
}

fn runtime_status() -> Option<String> {
    app_state()
        .transport_status
        .iter()
        .find(|status| status.label == "text/control runtime")
        .map(|status| status.status.clone())
}

fn ensure_ok<T: std::fmt::Debug>(
    error: &Option<T>,
    label: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(error) = error {
        return Err(format!("{label} failed: {error:?}").into());
    }
    Ok(())
}

fn parse_args() -> Result<Args, Box<dyn std::error::Error>> {
    let mut role = None;
    let mut artifact = None;
    let mut invite = env::var("DISCRYPT_G009_INVITE").ok();
    let mut adapter = "nostr".to_owned();
    let mut endpoint = "wss://nos.lol".to_owned();
    let mut group_name = "G009 Split Machine Lab".to_owned();
    let mut timeout_secs = 120_u64;
    let mut iter = env::args().skip(1);
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--role" => role = iter.next(),
            "--artifact" => artifact = iter.next().map(PathBuf::from),
            "--invite" => invite = iter.next(),
            "--adapter" => adapter = iter.next().ok_or("--adapter requires value")?,
            "--endpoint" => endpoint = iter.next().ok_or("--endpoint requires value")?,
            "--group-name" => group_name = iter.next().ok_or("--group-name requires value")?,
            "--timeout-secs" => {
                timeout_secs = iter
                    .next()
                    .ok_or("--timeout-secs requires value")?
                    .parse()?
            }
            "--help" | "-h" => {
                println!("usage: g009_split_machine_app_flow --role prepare-owner|owner|joiner --artifact <path> [--invite <invite>] [--adapter nostr|mqtt] [--endpoint <uri>]");
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}").into()),
        }
    }
    Ok(Args {
        role: role.ok_or("--role is required")?,
        artifact: artifact
            .unwrap_or_else(|| PathBuf::from("target/g009-split-machine-app-flow.json")),
        invite,
        adapter,
        endpoint,
        group_name,
        timeout_secs,
    })
}
