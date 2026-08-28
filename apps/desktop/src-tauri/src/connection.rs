//! Backend-owned broker control lane session manager.
//!
//! Owns a supervisor thread that pumps pending text/control frames and drains
//! sealed inbound control frames on a schedule, so delivery never depends on a
//! visible UI screen or user-driven pump commands. Transport failures back off
//! through the deterministic abuse-control policy and stop fail-closed after
//! exhausted attempts. The manager never holds the app-service lock across
//! sleeps: each drive cycle locks, pumps, drains, and releases.

use super::{
    app_service, redacted_observable_ref, AppStateView, BackendTransportMode,
    ListPendingTextControlFramesRequest, TauriAppService,
};
use discrypt_abuse::{AbuseBackoffPolicy, AbuseDecision};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

const DEFAULT_PUMP_INTERVAL_MS: u64 = 1_000;
const DEFAULT_MANAGER_DRAIN_MS: u64 = 300;
const DEFAULT_BACKOFF_INITIAL_MS: u64 = 250;
const DEFAULT_BACKOFF_MAX_MS: u64 = 5_000;
const DEFAULT_BACKOFF_MULTIPLIER: u64 = 2;
const DEFAULT_BACKOFF_MAX_ATTEMPTS: u32 = 5;

/// One pump+drain cycle result reported by a session driver.
pub struct ControlLaneDriveReport {
    /// Frames the pump handed to the attached transport.
    pub frames_sent: usize,
    /// Inbound control frames drained and applied to app state.
    pub inbound_applied: usize,
    /// Combined pump/drain failures for backoff classification.
    pub failures: Vec<String>,
}

/// Drives one pump+drain cycle for the managed session.
pub trait ControlLaneSessionDriver: Send + 'static {
    /// Run one bounded pump+drain cycle.
    fn drive_once(&self, drain_ms: u64) -> ControlLaneDriveReport;

    /// Record a fail-closed stop with an actionable reason.
    fn on_manager_stopped(&self, reason: &str);
}

fn drive_service_once(service: &mut TauriAppService, drain_ms: u64) -> ControlLaneDriveReport {
    let pump = service.pump_text_control_transport_once(ListPendingTextControlFramesRequest {
        target: None,
        limit: Some(16),
        operation_timeout_ms: Some(5_000),
    });
    let drain = service.drain_text_control_inbound_frames(Some(drain_ms), Some(2_000));
    let mut failures = pump.failures;
    failures.extend(drain.failures);
    ControlLaneDriveReport {
        frames_sent: pump.frames_sent,
        inbound_applied: drain.response_frames_received,
        failures,
    }
}

/// Driver that pumps and drains the process-global app service.
pub struct GlobalControlLaneSessionDriver;

impl ControlLaneSessionDriver for GlobalControlLaneSessionDriver {
    fn drive_once(&self, drain_ms: u64) -> ControlLaneDriveReport {
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drive_service_once(&mut guard, drain_ms)
    }

    fn on_manager_stopped(&self, reason: &str) {
        let service = app_service();
        let mut guard = service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.state.push_event(
            "transport.control_session_fail_closed",
            format!("Broker control lane session manager stopped fail-closed: {reason}"),
        );
        guard.persist();
    }
}

/// Driver over a shared non-global app service (subprocess harnesses, tests).
pub struct SharedControlLaneSessionDriver {
    pub(crate) service: Arc<Mutex<TauriAppService>>,
}

impl ControlLaneSessionDriver for SharedControlLaneSessionDriver {
    fn drive_once(&self, drain_ms: u64) -> ControlLaneDriveReport {
        let mut guard = self
            .service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        drive_service_once(&mut guard, drain_ms)
    }

    fn on_manager_stopped(&self, reason: &str) {
        let mut guard = self
            .service
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guard.state.push_event(
            "transport.control_session_fail_closed",
            format!("Broker control lane session manager stopped fail-closed: {reason}"),
        );
        guard.persist();
    }
}

/// Tunables for one managed control lane session.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlLaneManagerConfig {
    /// Delay between drive cycles. Clamped to 50..=60000, defaults to 1000ms.
    #[serde(default)]
    pub pump_interval_ms: Option<u64>,
    /// Inbound drain window per cycle. Clamped to 100..=5000, defaults to 300ms.
    #[serde(default)]
    pub drain_ms: Option<u64>,
    /// Backoff initial delay. Clamped to 10..=60000, defaults to 250ms.
    #[serde(default)]
    pub backoff_initial_ms: Option<u64>,
    /// Backoff maximum delay. Clamped to 10..=60000, defaults to 5000ms.
    #[serde(default)]
    pub backoff_max_ms: Option<u64>,
    /// Backoff attempts before fail-closed stop. Defaults to 5.
    #[serde(default)]
    pub backoff_max_attempts: Option<u32>,
}

struct ControlLaneManagerEntry {
    stop: Arc<AtomicBool>,
    iterations: Arc<AtomicU64>,
    consecutive_failures: Arc<AtomicU64>,
    stopped: Arc<AtomicBool>,
}

/// Observable manager state for one session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlLaneSessionManagerStatusView {
    /// Managed text/control transport session id.
    pub session_id: String,
    /// True while the manager is registered and running.
    pub running: bool,
    /// Completed drive cycles.
    pub iterations: u64,
    /// Consecutive failed cycles before the last success.
    pub consecutive_failures: u64,
    /// True once the manager stopped (explicitly or fail-closed).
    pub stopped: bool,
}

/// Request to start the backend-owned control lane session manager.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StartControlLaneSessionManagerRequest {
    /// Delay between drive cycles. Clamped to 50..=60000, defaults to 1000ms.
    #[serde(default)]
    pub pump_interval_ms: Option<u64>,
    /// Inbound drain window per cycle. Clamped to 100..=5000, defaults to 300ms.
    #[serde(default)]
    pub drain_ms: Option<u64>,
    /// Backoff initial delay. Clamped to 10..=60000, defaults to 250ms.
    #[serde(default)]
    pub backoff_initial_ms: Option<u64>,
    /// Backoff maximum delay. Clamped to 10..=60000, defaults to 5000ms.
    #[serde(default)]
    pub backoff_max_ms: Option<u64>,
    /// Backoff attempts before fail-closed stop. Defaults to 5.
    #[serde(default)]
    pub backoff_max_attempts: Option<u32>,
}

/// Request to stop the backend-owned control lane session manager.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct StopControlLaneSessionManagerRequest {
    /// Session to stop; defaults to the active text transport session.
    #[serde(default)]
    pub session_id: Option<String>,
}

/// Request for control lane session manager status.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct ControlLaneSessionManagerStatusRequest {
    /// Session to inspect; defaults to the active text transport session.
    #[serde(default)]
    pub session_id: Option<String>,
}

static CONTROL_LANE_MANAGERS: OnceLock<Mutex<BTreeMap<String, ControlLaneManagerEntry>>> =
    OnceLock::new();

fn managers() -> &'static Mutex<BTreeMap<String, ControlLaneManagerEntry>> {
    CONTROL_LANE_MANAGERS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn sleep_interruptible(stop: &AtomicBool, budget: Duration) {
    let deadline = Instant::now() + budget;
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        std::thread::sleep(remaining.min(Duration::from_millis(25)));
    }
}

/// Spawn the backend-owned supervisor thread for one control lane session.
///
/// Replaces any existing manager for the same session. Transport failures back
/// off through the deterministic abuse-control policy; exhausting the backoff
/// stops the manager fail-closed and records the stop in backend events.
pub fn spawn_control_lane_session_manager(
    session_id: String,
    driver: Box<dyn ControlLaneSessionDriver>,
    config: ControlLaneManagerConfig,
) -> Result<(), String> {
    halt_control_lane_session_manager(&session_id);
    let pump_interval = Duration::from_millis(
        config
            .pump_interval_ms
            .unwrap_or(DEFAULT_PUMP_INTERVAL_MS)
            .clamp(50, 60_000),
    );
    let drain_window = config.drain_ms.unwrap_or(DEFAULT_MANAGER_DRAIN_MS).clamp(100, 5_000);
    let backoff = AbuseBackoffPolicy::new(
        config
            .backoff_initial_ms
            .unwrap_or(DEFAULT_BACKOFF_INITIAL_MS)
            .clamp(10, 60_000),
        config
            .backoff_max_ms
            .unwrap_or(DEFAULT_BACKOFF_MAX_MS)
            .clamp(10, 60_000),
        DEFAULT_BACKOFF_MULTIPLIER,
        config.backoff_max_attempts.unwrap_or(DEFAULT_BACKOFF_MAX_ATTEMPTS).max(1),
    )
    .map_err(|error| error.to_string())?;
    let entry = ControlLaneManagerEntry {
        stop: Arc::new(AtomicBool::new(false)),
        iterations: Arc::new(AtomicU64::new(0)),
        consecutive_failures: Arc::new(AtomicU64::new(0)),
        stopped: Arc::new(AtomicBool::new(false)),
    };
    let stop = entry.stop.clone();
    let iterations = entry.iterations.clone();
    let consecutive_failures = entry.consecutive_failures.clone();
    let stopped = entry.stopped.clone();
    std::thread::Builder::new()
        .name(format!("discrypt-control-lane-{session_id}"))
        .stack_size(16 * 1024 * 1024)
        .spawn(move || {
            let mut attempt: u32 = 0;
            while !stop.load(Ordering::Relaxed) {
                iterations.fetch_add(1, Ordering::Relaxed);
                let report = driver.drive_once(drain_window);
                if report.failures.is_empty() {
                    attempt = 0;
                    consecutive_failures.store(0, Ordering::Relaxed);
                } else {
                    attempt = attempt.saturating_add(1);
                    consecutive_failures.store(u64::from(attempt), Ordering::Relaxed);
                    match backoff.decision_for_attempt(attempt) {
                        Ok(AbuseDecision::Backoff { delay_ms, .. })
                        | Ok(AbuseDecision::RateLimited {
                            retry_after_ms: delay_ms,
                        }) => {
                            sleep_interruptible(&stop, Duration::from_millis(delay_ms));
                            continue;
                        }
                        _ => {
                            driver.on_manager_stopped("backoff_exhausted");
                            break;
                        }
                    }
                }
                sleep_interruptible(&stop, pump_interval);
            }
            stopped.store(true, Ordering::Relaxed);
        })
        .map_err(|error| format!("could not spawn control lane session manager: {error}"))?;
    managers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(session_id, entry);
    Ok(())
}

/// Stop the manager for one session. Returns true when a manager was registered.
pub fn halt_control_lane_session_manager(session_id: &str) -> bool {
    let entry = managers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(session_id);
    let Some(entry) = entry else {
        return false;
    };
    entry.stop.store(true, Ordering::Relaxed);
    true
}

/// Snapshot manager state for one session.
#[must_use]
pub fn control_lane_session_manager_snapshot(
    session_id: &str,
) -> Option<ControlLaneSessionManagerStatusView> {
    let guard = managers()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.get(session_id).map(|entry| {
        let stopped = entry.stopped.load(Ordering::Relaxed);
        ControlLaneSessionManagerStatusView {
            session_id: session_id.to_owned(),
            running: !stopped && !entry.stop.load(Ordering::Relaxed),
            iterations: entry.iterations.load(Ordering::Relaxed),
            consecutive_failures: entry.consecutive_failures.load(Ordering::Relaxed),
            stopped,
        }
    })
}

fn active_text_session_id() -> Option<String> {
    let service = app_service();
    let guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard
        .state
        .transport_session(BackendTransportMode::Text)
        .map(|session| session.session_id.clone())
}

/// Tauri command: start the backend-owned broker control lane session manager.
pub fn start_control_lane_session_manager(
    request: StartControlLaneSessionManagerRequest,
) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(session_id) = active_text_session_id() else {
        guard.state.push_command_error(
            "transport.control_session_start_rejected",
            "start_control_lane_session_manager",
            "text_session_missing",
            "text transport session is not active",
            "Call start_text_session and attach the broker control lane before starting the session manager",
        );
        guard.persist();
        return guard.to_view();
    };
    match spawn_control_lane_session_manager(
        session_id.clone(),
        Box::new(GlobalControlLaneSessionDriver),
        ControlLaneManagerConfig {
            pump_interval_ms: request.pump_interval_ms,
            drain_ms: request.drain_ms,
            backoff_initial_ms: request.backoff_initial_ms,
            backoff_max_ms: request.backoff_max_ms,
            backoff_max_attempts: request.backoff_max_attempts,
        },
    ) {
        Ok(()) => {
            guard.state.push_event(
                "transport.control_session_started",
                format!(
                    "Broker control lane session manager started for {}",
                    redacted_observable_ref("session", &session_id)
                ),
            );
        }
        Err(error) => {
            guard.state.push_command_error(
                "transport.control_session_start_rejected",
                "start_control_lane_session_manager",
                "control_session_start_failed",
                error,
                "Retry after the backend runtime can spawn the control lane session manager",
            );
        }
    }
    guard.persist();
    guard.to_view()
}

/// Tauri command: stop the backend-owned broker control lane session manager.
pub fn stop_control_lane_session_manager(
    request: StopControlLaneSessionManagerRequest,
) -> AppStateView {
    let service = app_service();
    let mut guard = service
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session_id = request.session_id.or_else(active_text_session_id);
    let Some(session_id) = session_id else {
        guard.state.push_event(
            "transport.control_session_stop_noop",
            "No active text transport session; no control lane session manager to stop".to_owned(),
        );
        guard.persist();
        return guard.to_view();
    };
    if halt_control_lane_session_manager(&session_id) {
        guard.state.push_event(
            "transport.control_session_stopped",
            format!(
                "Broker control lane session manager stopped for {}",
                redacted_observable_ref("session", &session_id)
            ),
        );
    } else {
        guard.state.push_event(
            "transport.control_session_stop_noop",
            format!(
                "No control lane session manager registered for {}",
                redacted_observable_ref("session", &session_id)
            ),
        );
    }
    guard.persist();
    guard.to_view()
}

/// Tauri command: inspect control lane session manager state.
pub fn control_lane_session_manager_status(
    request: ControlLaneSessionManagerStatusRequest,
) -> Option<ControlLaneSessionManagerStatusView> {
    let session_id = request
        .session_id
        .or_else(active_text_session_id)?;
    control_lane_session_manager_snapshot(&session_id)
}
