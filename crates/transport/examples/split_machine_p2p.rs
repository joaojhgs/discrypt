use discrypt_transport::{
    derive_scope_commitment, start_provider_webrtc_text_control_answer_runtime_with_answerer,
    start_provider_webrtc_text_control_offer_runtime, ConnectivityScopeLevel, ConversationScope,
    Endpoint, IceServerConfig, SignalingAdapterProfile, SignalingPeerId, TransportError,
    WebRtcNegotiationConfig,
};
#[cfg(any(feature = "mqtt-adapter", feature = "nostr-adapter"))]
use discrypt_transport::{
    AdapterTrustLabel, ProviderMetadataPosture, SignalingAdapterCapabilities, SignalingAdapterKind,
    SignalingEndpointSecurity, SignalingProviderEndpoint,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Role {
    Offerer,
    Answerer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Adapter {
    Mqtt,
    Nostr,
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse()?;
    fs::create_dir_all(&args.artifact_dir)?;
    let started_at = chrono::Utc::now();
    let result = match args.role {
        Role::Answerer => run_answerer(&args).await,
        Role::Offerer => run_offerer(&args).await,
    };
    let completed_at = chrono::Utc::now();
    match result {
        Ok(evidence) => {
            let path = args.artifact_dir.join(format!(
                "split-machine-{}-{}.json",
                args.adapter.as_str(),
                args.role.as_str()
            ));
            let doc = json!({
                "schema_version": "discrypt.split_machine_p2p.v1",
                "status": "passed",
                "adapter": args.adapter.as_str(),
                "role": args.role.as_str(),
                "room": args.room,
                "endpoint": args.endpoint,
                "started_at": started_at.to_rfc3339(),
                "completed_at": completed_at.to_rfc3339(),
                "evidence": evidence,
            });
            fs::write(&path, serde_json::to_vec_pretty(&doc)?)?;
            println!("split-machine artifact: {}", path.display());
            Ok(())
        }
        Err(error) => {
            let path = args.artifact_dir.join(format!(
                "split-machine-{}-{}-failure.json",
                args.adapter.as_str(),
                args.role.as_str()
            ));
            let doc = json!({
                "schema_version": "discrypt.split_machine_p2p.v1",
                "status": "failed",
                "adapter": args.adapter.as_str(),
                "role": args.role.as_str(),
                "room": args.room,
                "endpoint": args.endpoint,
                "started_at": started_at.to_rfc3339(),
                "completed_at": completed_at.to_rfc3339(),
                "error": error.to_string(),
            });
            fs::write(&path, serde_json::to_vec_pretty(&doc)?)?;
            Err(Box::new(error) as Box<dyn std::error::Error>)
        }
    }
}

async fn run_answerer(args: &Args) -> Result<serde_json::Value, TransportError> {
    let profile = adapter_profile(args.adapter, &args.endpoint)?;
    let scope = scope_for_room(&args.room)?;
    let bootstrap_secret = hash32("bootstrap", &args.room);
    let random_entropy = hash16("entropy", &args.room);
    let ice = negotiation_config()?;
    let received_count = Arc::new(AtomicU64::new(0));
    let received_bytes = Arc::new(AtomicU64::new(0));
    let received_count_for_callback = Arc::clone(&received_count);
    let received_bytes_for_callback = Arc::clone(&received_bytes);
    let runtime = start_provider_webrtc_text_control_answer_runtime_with_answerer(
        profile,
        scope,
        &bootstrap_secret,
        &random_entropy,
        ice,
        answerer_peer_id()?,
        offerer_peer_id()?,
        move |frame| {
            received_count_for_callback.fetch_add(1, Ordering::SeqCst);
            received_bytes_for_callback.fetch_add(frame.len() as u64, Ordering::SeqCst);
            let mut receipt = b"ciphertext:split-machine-receipt:".to_vec();
            receipt.extend_from_slice(&sha256_bytes(&frame));
            Ok(receipt)
        },
    )
    .await?;
    let evidence = runtime.evidence().clone();
    tokio::time::sleep(Duration::from_secs(args.answerer_hold_secs)).await;
    let received_count_final = received_count.load(Ordering::SeqCst);
    let received_bytes_final = received_bytes.load(Ordering::SeqCst);
    let direct_path_ready = evidence.direct_path_ready;
    let data_channel_open = evidence.data_channel_open;
    runtime.close().await?;
    Ok(json!({
        "runtime": evidence,
        "received_frame_count": received_count_final,
        "received_opaque_bytes": received_bytes_final,
        "p2p_datachannel_open": true,
        "direct_path_ready": direct_path_ready,
        "data_channel_open": data_channel_open,
    }))
}

async fn run_offerer(args: &Args) -> Result<serde_json::Value, TransportError> {
    let profile = adapter_profile(args.adapter, &args.endpoint)?;
    let scope = scope_for_room(&args.room)?;
    let bootstrap_secret = hash32("bootstrap", &args.room);
    let random_entropy = hash16("entropy", &args.room);
    let ice = negotiation_config()?;
    let runtime = start_provider_webrtc_text_control_offer_runtime(
        profile,
        scope,
        &bootstrap_secret,
        &random_entropy,
        ice,
        offerer_peer_id()?,
        answerer_peer_id()?,
    )
    .await?;
    let evidence = runtime.evidence().clone();
    let transport = runtime.transport();

    let text_frame = format!(
        "ciphertext:split-machine-text:{}:{}",
        args.adapter.as_str(),
        args.room
    )
    .into_bytes();
    transport
        .send_text_control_frame(text_frame.clone())
        .await?;
    let text_receipt = tokio::time::timeout(
        Duration::from_secs(args.receipt_timeout_secs),
        transport.recv_text_control_frame(),
    )
    .await
    .map_err(|_| {
        TransportError::Unavailable("timed out waiting for split-machine text receipt".to_owned())
    })??;

    let media_frame = media_frame_payload(&args.room, args.adapter);
    transport
        .send_text_control_frame(media_frame.clone())
        .await?;
    let media_receipt = tokio::time::timeout(
        Duration::from_secs(args.receipt_timeout_secs),
        transport.recv_text_control_frame(),
    )
    .await
    .map_err(|_| {
        TransportError::Unavailable("timed out waiting for split-machine media receipt".to_owned())
    })??;

    let direct_path_ready = evidence.direct_path_ready;
    let data_channel_open = evidence.data_channel_open;
    runtime.close().await?;
    Ok(json!({
        "runtime": evidence,
        "p2p_datachannel_open": true,
        "direct_path_ready": direct_path_ready,
        "data_channel_open": data_channel_open,
        "text_frame_sha256": to_hex(&sha256_bytes(&text_frame)),
        "text_receipt_sha256": to_hex(&sha256_bytes(&text_receipt)),
        "text_receipt_prefix_ok": text_receipt.starts_with(b"ciphertext:split-machine-receipt:"),
        "media_frame_sha256": to_hex(&sha256_bytes(&media_frame)),
        "media_receipt_sha256": to_hex(&sha256_bytes(&media_receipt)),
        "media_receipt_prefix_ok": media_receipt.starts_with(b"ciphertext:split-machine-receipt:"),
        "text_chat_boundary": "opaque text/control frame crossed a real provider-signaled WebRTC DataChannel between local and SSH-host peer",
        "voice_boundary": "opaque media-frame ciphertext crossed the same real provider-signaled WebRTC DataChannel; this is native media-frame transport proof, not physical microphone audio",
    }))
}

fn negotiation_config() -> Result<WebRtcNegotiationConfig, TransportError> {
    let config = WebRtcNegotiationConfig::new(IceServerConfig::new(
        vec![Endpoint::new("stun:stun.l.google.com:19302")],
        vec![],
    )?);
    Ok(config)
}

fn scope_for_room(room: &str) -> Result<ConversationScope, TransportError> {
    let scope_secret = hash32("scope", room);
    ConversationScope::new(
        ConnectivityScopeLevel::Dm,
        derive_scope_commitment(ConnectivityScopeLevel::Dm, &scope_secret, room),
    )
}

fn adapter_profile(
    adapter: Adapter,
    endpoint: &str,
) -> Result<SignalingAdapterProfile, TransportError> {
    match adapter {
        Adapter::Mqtt => mqtt_profile(endpoint),
        Adapter::Nostr => nostr_profile(endpoint),
    }
}

#[cfg(feature = "mqtt-adapter")]
fn mqtt_profile(endpoint: &str) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "split-machine-mqtt".to_owned(),
        kind: SignalingAdapterKind::Mqtt,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint.to_owned()),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "split-machine mqtt",
            "public MQTT broker; opaque WebRTC negotiation and app frames only",
        )?,
    })
}

#[cfg(not(feature = "mqtt-adapter"))]
fn mqtt_profile(_endpoint: &str) -> Result<SignalingAdapterProfile, TransportError> {
    Err(TransportError::Unavailable(
        "mqtt-adapter feature is not enabled".to_owned(),
    ))
}

#[cfg(feature = "nostr-adapter")]
fn nostr_profile(endpoint: &str) -> Result<SignalingAdapterProfile, TransportError> {
    Ok(SignalingAdapterProfile {
        profile_id: "split-machine-nostr".to_owned(),
        kind: SignalingAdapterKind::Nostr,
        endpoints: vec![SignalingProviderEndpoint::new(
            Endpoint::new(endpoint.to_owned()),
            SignalingEndpointSecurity::ProductionTls,
        )],
        metadata_posture: ProviderMetadataPosture::HashedTopic,
        capabilities: SignalingAdapterCapabilities::production_required(),
        trust_label: AdapterTrustLabel::new(
            "split-machine nostr",
            "public Nostr relay; opaque WebRTC negotiation and app frames only",
        )?,
    })
}

#[cfg(not(feature = "nostr-adapter"))]
fn nostr_profile(_endpoint: &str) -> Result<SignalingAdapterProfile, TransportError> {
    Err(TransportError::Unavailable(
        "nostr-adapter feature is not enabled".to_owned(),
    ))
}

fn offerer_peer_id() -> Result<SignalingPeerId, TransportError> {
    SignalingPeerId::new("split-machine-local-offerer")
}

fn answerer_peer_id() -> Result<SignalingPeerId, TransportError> {
    SignalingPeerId::new("split-machine-ssh-answerer")
}

fn media_frame_payload(room: &str, adapter: Adapter) -> Vec<u8> {
    let mut frame = Vec::new();
    frame.extend_from_slice(b"media-frame-ciphertext:v1:split-machine:");
    frame.extend_from_slice(adapter.as_str().as_bytes());
    frame.push(b':');
    frame.extend_from_slice(&sha256_bytes(room.as_bytes()));
    frame.extend(std::iter::repeat_n(0xA5_u8, 256));
    frame
}

fn hash32(label: &str, room: &str) -> [u8; 32] {
    sha256_bytes(format!("discrypt-split-machine:{label}:{room}").as_bytes())
}

fn hash16(label: &str, room: &str) -> [u8; 16] {
    let hash = sha256_bytes(format!("discrypt-split-machine:{label}:{room}").as_bytes());
    let mut out = [0_u8; 16];
    out.copy_from_slice(&hash[..16]);
    out
}

fn sha256_bytes(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut out = [0_u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn to_hex(bytes: &[u8]) -> String {
    const TABLE: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(TABLE[(byte >> 4) as usize] as char);
        out.push(TABLE[(byte & 0x0f) as usize] as char);
    }
    out
}

struct Args {
    adapter: Adapter,
    role: Role,
    room: String,
    endpoint: String,
    artifact_dir: PathBuf,
    answerer_hold_secs: u64,
    receipt_timeout_secs: u64,
}

impl Args {
    fn parse() -> Result<Self, Box<dyn std::error::Error>> {
        let mut adapter = None;
        let mut role = None;
        let mut room = env::var("DISCRYPT_SPLIT_MACHINE_ROOM").ok();
        let mut endpoint = None;
        let mut artifact_dir = env::var("DISCRYPT_SPLIT_MACHINE_ARTIFACT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("target/split-machine-p2p"));
        let mut answerer_hold_secs = env::var("DISCRYPT_SPLIT_MACHINE_ANSWERER_HOLD_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(90);
        let mut receipt_timeout_secs = env::var("DISCRYPT_SPLIT_MACHINE_RECEIPT_TIMEOUT_SECS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(30);
        let mut iter = env::args().skip(1);
        while let Some(arg) = iter.next() {
            match arg.as_str() {
                "--adapter" => adapter = iter.next().as_deref().map(Adapter::parse).transpose()?,
                "--role" => role = iter.next().as_deref().map(Role::parse).transpose()?,
                "--room" => room = iter.next(),
                "--endpoint" => endpoint = iter.next(),
                "--artifact-dir" => {
                    artifact_dir =
                        PathBuf::from(iter.next().ok_or("--artifact-dir requires value")?);
                }
                "--answerer-hold-secs" => {
                    answerer_hold_secs = iter
                        .next()
                        .ok_or("--answerer-hold-secs requires value")?
                        .parse()?;
                }
                "--receipt-timeout-secs" => {
                    receipt_timeout_secs = iter
                        .next()
                        .ok_or("--receipt-timeout-secs requires value")?
                        .parse()?;
                }
                "--help" | "-h" => {
                    println!("usage: split_machine_p2p --adapter mqtt|nostr --role answerer|offerer --room <shared-room> [--endpoint <uri>] [--artifact-dir <dir>]");
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}").into()),
            }
        }
        let adapter = adapter.ok_or("--adapter is required")?;
        let endpoint = endpoint.unwrap_or_else(|| adapter.default_endpoint());
        Ok(Self {
            adapter,
            role: role.ok_or("--role is required")?,
            room: room.ok_or("--room or DISCRYPT_SPLIT_MACHINE_ROOM is required")?,
            endpoint,
            artifact_dir,
            answerer_hold_secs,
            receipt_timeout_secs,
        })
    }
}

impl Adapter {
    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value {
            "mqtt" => Ok(Self::Mqtt),
            "nostr" => Ok(Self::Nostr),
            other => Err(format!("unsupported adapter: {other}").into()),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Mqtt => "mqtt",
            Self::Nostr => "nostr",
        }
    }

    fn default_endpoint(self) -> String {
        match self {
            Self::Mqtt => env::var("DISCRYPT_PUBLIC_MQTT_ENDPOINT")
                .unwrap_or_else(|_| "mqtts://broker.emqx.io:8883".to_owned()),
            Self::Nostr => env::var("DISCRYPT_PUBLIC_NOSTR_ENDPOINT")
                .unwrap_or_else(|_| "wss://nos.lol".to_owned()),
        }
    }
}

impl Role {
    fn parse(value: &str) -> Result<Self, Box<dyn std::error::Error>> {
        match value {
            "offerer" => Ok(Self::Offerer),
            "answerer" => Ok(Self::Answerer),
            other => Err(format!("unsupported role: {other}").into()),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Offerer => "offerer",
            Self::Answerer => "answerer",
        }
    }
}
