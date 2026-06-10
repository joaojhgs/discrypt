//! Transport session state machine for ICE/WebRTC negotiation and route status.
//!
//! This module deliberately models session state and status only. It does not
//! perform signaling, ICE gathering, candidate exchange, or WebRTC transport I/O;
//! later Phase-G work can drive this typed state machine from those production
//! integrations.

use crate::{ConnectivityPlan, Endpoint, FallbackLeg, RouteReport};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Lifecycle state for one transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TransportSessionState {
    /// No negotiation has started.
    Idle,
    /// Offer/answer or equivalent signaling is in progress.
    Signaling,
    /// ICE candidates are being gathered.
    IceGathering,
    /// Connectivity checks are running against gathered candidates.
    Checking,
    /// A direct ICE/STUN route is active.
    Direct,
    /// Legacy unsupported peer overlay route. Retained only for decoding old snapshots.
    OverlayRelay,
    /// The encrypted TURN route is active.
    TurnRelay,
    /// A previously active route was lost.
    Disconnected,
    /// Reconnection orchestration is in progress.
    Reconnecting,
    /// The session has reached a terminal failure until reset.
    Failed,
    /// Session was intentionally cancelled or torn down locally.
    Cancelled,
}

impl TransportSessionState {
    /// The exact state set promised by the G041 transport-session contract.
    pub const ALL: [Self; 11] = [
        Self::Idle,
        Self::Signaling,
        Self::IceGathering,
        Self::Checking,
        Self::Direct,
        Self::OverlayRelay,
        Self::TurnRelay,
        Self::Disconnected,
        Self::Reconnecting,
        Self::Failed,
        Self::Cancelled,
    ];

    /// True when the state represents an active data route.
    #[must_use]
    pub const fn is_connected(self) -> bool {
        matches!(self, Self::Direct | Self::TurnRelay)
    }
}

/// Route kind selected by the transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TransportRoute {
    /// Direct ICE/STUN connectivity.
    Direct,
    /// Legacy unsupported peer overlay fallback. The current policy never selects it.
    OverlayRelay,
    /// Encrypted TURN fallback.
    TurnRelay,
}

impl TransportRoute {
    /// Convert a fallback planner leg into the route state surfaced by sessions.
    #[must_use]
    pub const fn from_fallback_leg(leg: FallbackLeg) -> Self {
        match leg {
            FallbackLeg::Stun => Self::Direct,
            FallbackLeg::RelayOverlay => Self::OverlayRelay,
            FallbackLeg::Turn => Self::TurnRelay,
        }
    }

    const fn state(self) -> TransportSessionState {
        match self {
            Self::Direct => TransportSessionState::Direct,
            Self::OverlayRelay => TransportSessionState::OverlayRelay,
            Self::TurnRelay => TransportSessionState::TurnRelay,
        }
    }
}

/// Typed transition requested against a transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TransportSessionEvent {
    /// Begin signaling from idle or reconnecting.
    StartSignaling,
    /// Move from signaling into ICE gathering.
    StartIceGathering,
    /// Move from ICE gathering into connectivity checks.
    StartConnectivityChecks,
    /// Select direct ICE/STUN connectivity.
    SelectDirect,
    /// Legacy unsupported peer overlay relay selection.
    SelectOverlayRelay,
    /// Select encrypted TURN connectivity.
    SelectTurnRelay,
    /// Mark an active route disconnected.
    MarkDisconnected,
    /// Begin reconnection after disconnection.
    StartReconnecting,
    /// Mark the session failed.
    MarkFailed,
    /// Cancel any pending reconnect or active route.
    Cancel,
    /// Tear down this session and release route state.
    TearDown,
    /// Reset a failed or disconnected session back to idle.
    Reset,
}

/// Deterministic reconnect backoff policy for one transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReconnectBackoffPolicy {
    /// Initial reconnect delay in milliseconds.
    pub initial_delay_ms: u64,
    /// Maximum reconnect delay in milliseconds.
    pub max_delay_ms: u64,
    /// Integer multiplier applied after each attempt.
    pub multiplier: u64,
    /// Maximum reconnect attempts before terminal failure.
    pub max_attempts: u32,
}

impl Default for ReconnectBackoffPolicy {
    fn default() -> Self {
        Self {
            initial_delay_ms: 250,
            max_delay_ms: 8_000,
            multiplier: 2,
            max_attempts: 8,
        }
    }
}

impl ReconnectBackoffPolicy {
    /// Build a policy after validating monotonic finite backoff fields.
    pub fn new(
        initial_delay_ms: u64,
        max_delay_ms: u64,
        multiplier: u64,
        max_attempts: u32,
    ) -> Result<Self, TransportSessionError> {
        let policy = Self {
            initial_delay_ms,
            max_delay_ms,
            multiplier,
            max_attempts,
        };
        policy.validate()?;
        Ok(policy)
    }

    fn validate(self) -> Result<(), TransportSessionError> {
        if self.initial_delay_ms == 0
            || self.max_delay_ms < self.initial_delay_ms
            || self.multiplier < 1
            || self.max_attempts == 0
        {
            Err(TransportSessionError::InvalidReconnectPolicy)
        } else {
            Ok(())
        }
    }

    /// Delay for the next one-based reconnect attempt.
    #[must_use]
    pub fn delay_for_attempt(self, attempt: u32) -> u64 {
        let exponent = attempt.saturating_sub(1);
        let mut delay = self.initial_delay_ms;
        for _ in 0..exponent {
            delay = delay.saturating_mul(self.multiplier).min(self.max_delay_ms);
        }
        delay.min(self.max_delay_ms)
    }
}

/// Decision returned when scheduling a reconnect attempt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ReconnectDecision {
    /// One-based reconnect attempt number.
    pub attempt: u32,
    /// Backoff delay before the caller should start the next attempt.
    pub delay_ms: u64,
}

/// Active route details included in status snapshots.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportRouteStatus {
    /// Active route kind.
    pub route: TransportRoute,
    /// Endpoint or route descriptor selected for this session.
    pub endpoint: Endpoint,
    /// Planner report that selected the route, when selection came from a planner.
    pub route_report: Option<RouteReport>,
}

/// Serializable transport-session status for later Tauri/UI integration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportSessionSnapshot {
    /// Snapshot schema version for UI/Tauri consumers.
    pub schema_version: u16,
    /// Current lifecycle state.
    pub state: TransportSessionState,
    /// Active route details when connected.
    pub route: Option<TransportRouteStatus>,
    /// Number of reconnection attempts started since the last reset.
    pub reconnect_attempts: u32,
    /// Last failure or disconnection reason surfaced by the state machine.
    pub last_error: Option<String>,
}

impl TransportSessionSnapshot {
    /// Current snapshot schema version.
    pub const SCHEMA_VERSION: u16 = 1;

    /// True when the snapshot represents an active data route.
    #[must_use]
    pub fn connected(&self) -> bool {
        self.state.is_connected() && self.route.is_some()
    }
}

/// Error returned by invalid transport-session transitions.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum TransportSessionError {
    /// The requested transition is not valid from the current state.
    #[error("invalid transport session transition from {from:?} on {event:?}")]
    InvalidTransition {
        /// Current state when the transition was requested.
        from: TransportSessionState,
        /// Requested event.
        event: TransportSessionEvent,
    },
    /// The route is not supported by the current fail-closed transport policy.
    #[error("unsupported transport route under current policy: {route:?}")]
    UnsupportedRoute {
        /// Rejected route.
        route: TransportRoute,
    },
    /// Reconnect backoff policy is malformed.
    #[error("invalid reconnect backoff policy")]
    InvalidReconnectPolicy,
    /// Reconnect was requested after the policy attempt budget was exhausted.
    #[error("reconnect attempts exhausted after {attempts} attempts")]
    ReconnectAttemptsExhausted {
        /// Attempts already consumed.
        attempts: u32,
    },
}

/// Stateful transport session model for negotiation and route selection.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportSession {
    state: TransportSessionState,
    route: Option<TransportRouteStatus>,
    reconnect_attempts: u32,
    last_error: Option<String>,
}

impl Default for TransportSession {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportSession {
    /// Create a new idle transport session.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: TransportSessionState::Idle,
            route: None,
            reconnect_attempts: 0,
            last_error: None,
        }
    }

    /// Current lifecycle state.
    #[must_use]
    pub const fn state(&self) -> TransportSessionState {
        self.state
    }

    /// Build a serializable snapshot of the current session status.
    #[must_use]
    pub fn snapshot(&self) -> TransportSessionSnapshot {
        TransportSessionSnapshot {
            schema_version: TransportSessionSnapshot::SCHEMA_VERSION,
            state: self.state,
            route: self.route.clone(),
            reconnect_attempts: self.reconnect_attempts,
            last_error: self.last_error.clone(),
        }
    }

    /// Begin signaling from idle or reconnecting.
    pub fn begin_signaling(&mut self) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::StartSignaling,
            &[
                TransportSessionState::Idle,
                TransportSessionState::Reconnecting,
            ],
        )?;
        self.route = None;
        self.last_error = None;
        self.state = TransportSessionState::Signaling;
        Ok(self.snapshot())
    }

    /// Move from signaling to ICE gathering.
    pub fn begin_ice_gathering(
        &mut self,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::StartIceGathering,
            &[TransportSessionState::Signaling],
        )?;
        self.state = TransportSessionState::IceGathering;
        Ok(self.snapshot())
    }

    /// Move from ICE gathering to connectivity checks.
    pub fn begin_checking(&mut self) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::StartConnectivityChecks,
            &[TransportSessionState::IceGathering],
        )?;
        self.state = TransportSessionState::Checking;
        Ok(self.snapshot())
    }

    /// Select a direct ICE/STUN route from the checking state.
    pub fn select_direct(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.select_route(TransportRoute::Direct, endpoint, None)
    }

    /// Reject the legacy overlay relay route; only direct WebRTC or configured TURN is allowed.
    pub fn select_overlay_relay(
        &mut self,
        _endpoint: Endpoint,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        Err(TransportSessionError::UnsupportedRoute {
            route: TransportRoute::OverlayRelay,
        })
    }

    /// Select an encrypted TURN relay route from the checking state.
    pub fn select_turn_relay(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.select_route(TransportRoute::TurnRelay, endpoint, None)
    }

    /// Select the route produced by the fallback planner from the checking state.
    pub fn select_connectivity_plan(
        &mut self,
        plan: ConnectivityPlan,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        let route = TransportRoute::from_fallback_leg(plan.selected);
        let endpoint = plan.endpoint.clone();
        let report = plan.route_report();
        self.select_route(route, endpoint, Some(report))
    }

    /// Mark an active route disconnected.
    pub fn mark_disconnected(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::MarkDisconnected,
            &[
                TransportSessionState::Direct,
                TransportSessionState::OverlayRelay,
                TransportSessionState::TurnRelay,
            ],
        )?;
        self.route = None;
        self.last_error = Some(reason.into());
        self.state = TransportSessionState::Disconnected;
        Ok(self.snapshot())
    }

    /// Begin reconnection after a disconnection.
    pub fn begin_reconnecting(
        &mut self,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::StartReconnecting,
            &[TransportSessionState::Disconnected],
        )?;
        self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
        self.state = TransportSessionState::Reconnecting;
        Ok(self.snapshot())
    }

    /// Schedule the next reconnect attempt and return the deterministic backoff delay.
    pub fn schedule_reconnect(
        &mut self,
        policy: ReconnectBackoffPolicy,
    ) -> Result<ReconnectDecision, TransportSessionError> {
        policy.validate()?;
        self.require_state(
            TransportSessionEvent::StartReconnecting,
            &[TransportSessionState::Disconnected],
        )?;
        let next_attempt = self.reconnect_attempts.saturating_add(1);
        if next_attempt > policy.max_attempts {
            self.route = None;
            self.last_error = Some("reconnect attempts exhausted".to_owned());
            self.state = TransportSessionState::Failed;
            return Err(TransportSessionError::ReconnectAttemptsExhausted {
                attempts: self.reconnect_attempts,
            });
        }
        self.reconnect_attempts = next_attempt;
        self.state = TransportSessionState::Reconnecting;
        Ok(ReconnectDecision {
            attempt: next_attempt,
            delay_ms: policy.delay_for_attempt(next_attempt),
        })
    }

    /// Cancel this session from any non-terminal state.
    pub fn cancel(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        if matches!(
            self.state,
            TransportSessionState::Failed | TransportSessionState::Cancelled
        ) {
            return Err(self.invalid_transition(TransportSessionEvent::Cancel));
        }
        self.route = None;
        self.last_error = Some(reason.into());
        self.state = TransportSessionState::Cancelled;
        Ok(self.snapshot())
    }

    /// Tear down this session and release route state. Idempotent after cancellation.
    pub fn tear_down(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        if self.state == TransportSessionState::Failed {
            return Err(self.invalid_transition(TransportSessionEvent::TearDown));
        }
        self.route = None;
        self.last_error = Some(reason.into());
        self.state = TransportSessionState::Cancelled;
        Ok(self.snapshot())
    }

    /// Mark the session failed until it is reset.
    pub fn fail(
        &mut self,
        reason: impl Into<String>,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        if self.state == TransportSessionState::Failed {
            return Err(self.invalid_transition(TransportSessionEvent::MarkFailed));
        }
        self.route = None;
        self.last_error = Some(reason.into());
        self.state = TransportSessionState::Failed;
        Ok(self.snapshot())
    }

    /// Reset a failed or disconnected session back to idle.
    pub fn reset(&mut self) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.require_state(
            TransportSessionEvent::Reset,
            &[
                TransportSessionState::Disconnected,
                TransportSessionState::Failed,
                TransportSessionState::Cancelled,
            ],
        )?;
        self.state = TransportSessionState::Idle;
        self.route = None;
        self.reconnect_attempts = 0;
        self.last_error = None;
        Ok(self.snapshot())
    }

    fn select_route(
        &mut self,
        route: TransportRoute,
        endpoint: Endpoint,
        route_report: Option<RouteReport>,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        if route == TransportRoute::OverlayRelay {
            return Err(TransportSessionError::UnsupportedRoute { route });
        }
        self.require_state(
            match route {
                TransportRoute::Direct => TransportSessionEvent::SelectDirect,
                TransportRoute::OverlayRelay => TransportSessionEvent::SelectOverlayRelay,
                TransportRoute::TurnRelay => TransportSessionEvent::SelectTurnRelay,
            },
            &[TransportSessionState::Checking],
        )?;
        self.route = Some(TransportRouteStatus {
            route,
            endpoint,
            route_report,
        });
        self.last_error = None;
        self.state = route.state();
        Ok(self.snapshot())
    }

    fn require_state(
        &self,
        event: TransportSessionEvent,
        allowed: &[TransportSessionState],
    ) -> Result<(), TransportSessionError> {
        if allowed.contains(&self.state) {
            Ok(())
        } else {
            Err(self.invalid_transition(event))
        }
    }

    const fn invalid_transition(&self, event: TransportSessionEvent) -> TransportSessionError {
        TransportSessionError::InvalidTransition {
            from: self.state,
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectivityConfig, ConnectivityPlanner, SimulatedNat};

    fn ready_session() -> Result<TransportSession, TransportSessionError> {
        let mut session = TransportSession::new();
        session.begin_signaling()?;
        session.begin_ice_gathering()?;
        session.begin_checking()?;
        Ok(session)
    }

    fn assert_snapshot_is_serializable<T>()
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
    }

    #[test]
    fn exposes_exact_g041_state_set() {
        assert_eq!(
            TransportSessionState::ALL,
            [
                TransportSessionState::Idle,
                TransportSessionState::Signaling,
                TransportSessionState::IceGathering,
                TransportSessionState::Checking,
                TransportSessionState::Direct,
                TransportSessionState::OverlayRelay,
                TransportSessionState::TurnRelay,
                TransportSessionState::Disconnected,
                TransportSessionState::Reconnecting,
                TransportSessionState::Failed,
                TransportSessionState::Cancelled,
            ]
        );
    }

    #[test]
    fn direct_path_reaches_connected_snapshot() -> Result<(), TransportSessionError> {
        let mut session = ready_session()?;
        let snapshot = session.select_direct(Endpoint::new("stun:direct.example:3478"))?;

        assert_eq!(snapshot.state, TransportSessionState::Direct);
        assert!(snapshot.connected());
        assert_eq!(session.state(), TransportSessionState::Direct);
        assert_eq!(
            snapshot.route.map(|route| (route.route, route.endpoint)),
            Some((
                TransportRoute::Direct,
                Endpoint::new("stun:direct.example:3478")
            ))
        );
        assert_snapshot_is_serializable::<TransportSessionSnapshot>();
        Ok(())
    }

    #[test]
    fn configured_turn_path_follows_planner_selection() -> Result<(), Box<dyn std::error::Error>> {
        let turn_config = ConnectivityConfig {
            default_turn: Endpoint::new("turns:relay.example:5349"),
            ..ConnectivityConfig::default()
        };

        let mut turn = ready_session()?;
        let turn_plan = ConnectivityPlanner::plan(&turn_config, SimulatedNat::turn_only())?;
        let turn_snapshot = turn.select_connectivity_plan(turn_plan)?;
        assert_eq!(turn_snapshot.state, TransportSessionState::TurnRelay);
        assert_eq!(
            turn_snapshot.route.as_ref().map(|route| (
                route.route,
                route.route_report.as_ref().map(|report| report.selected)
            )),
            Some((TransportRoute::TurnRelay, Some(FallbackLeg::Turn)))
        );
        Ok(())
    }

    #[test]
    fn active_route_rejects_duplicate_session_start_or_second_route(
    ) -> Result<(), TransportSessionError> {
        let mut session = ready_session()?;
        session.select_direct(Endpoint::new("stun:direct.example:3478"))?;

        assert_eq!(
            session.begin_signaling(),
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Direct,
                event: TransportSessionEvent::StartSignaling,
            })
        );
        assert_eq!(
            session.select_turn_relay(Endpoint::new("turns:relay.example:5349")),
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Direct,
                event: TransportSessionEvent::SelectTurnRelay,
            })
        );
        assert_eq!(session.state(), TransportSessionState::Direct);
        Ok(())
    }

    #[test]
    fn disconnected_session_can_reconnect_and_choose_new_route() -> Result<(), TransportSessionError>
    {
        let mut session = ready_session()?;
        session.select_direct(Endpoint::new("stun:direct.example:3478"))?;

        let disconnected = session.mark_disconnected("candidate pair failed")?;
        assert_eq!(disconnected.state, TransportSessionState::Disconnected);
        assert_eq!(disconnected.route, None);
        assert_eq!(
            disconnected.last_error.as_deref(),
            Some("candidate pair failed")
        );

        let reconnecting = session.begin_reconnecting()?;
        assert_eq!(reconnecting.state, TransportSessionState::Reconnecting);
        assert_eq!(reconnecting.reconnect_attempts, 1);

        session.begin_signaling()?;
        session.begin_ice_gathering()?;
        session.begin_checking()?;
        let recovered = session.select_turn_relay(Endpoint::new("turns:relay.example:5349"))?;
        assert_eq!(recovered.state, TransportSessionState::TurnRelay);
        assert_eq!(recovered.reconnect_attempts, 1);
        Ok(())
    }

    #[test]
    fn snapshot_round_trips_as_stable_status_json() -> Result<(), Box<dyn std::error::Error>> {
        let mut session = ready_session()?;
        let snapshot = session.select_turn_relay(Endpoint::new("turns:relay.example:5349"))?;

        let value = serde_json::to_value(&snapshot)?;
        assert_eq!(
            value.get("schema_version"),
            Some(&serde_json::json!(TransportSessionSnapshot::SCHEMA_VERSION))
        );
        assert_eq!(value.get("state"), Some(&serde_json::json!("TurnRelay")));
        assert_eq!(
            value.pointer("/route/route"),
            Some(&serde_json::json!("TurnRelay"))
        );
        assert_eq!(value.get("last_error"), Some(&serde_json::Value::Null));

        let decoded: TransportSessionSnapshot = serde_json::from_value(value)?;
        assert_eq!(decoded, snapshot);
        Ok(())
    }

    #[test]
    fn invalid_transition_reports_state_and_event() {
        let mut session = TransportSession::new();
        let result = session.begin_checking();

        assert_eq!(
            result,
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Idle,
                event: TransportSessionEvent::StartConnectivityChecks,
            })
        );
        assert_eq!(session.state(), TransportSessionState::Idle);
    }

    #[test]
    fn failure_is_terminal_until_reset() -> Result<(), TransportSessionError> {
        let mut session = ready_session()?;
        let failed = session.fail("all candidate pairs failed")?;
        assert_eq!(failed.state, TransportSessionState::Failed);
        assert_eq!(failed.route, None);
        assert_eq!(
            failed.last_error.as_deref(),
            Some("all candidate pairs failed")
        );

        assert_eq!(
            session.fail("still failed"),
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Failed,
                event: TransportSessionEvent::MarkFailed,
            })
        );

        let idle = session.reset()?;
        assert_eq!(idle.state, TransportSessionState::Idle);
        assert_eq!(idle.reconnect_attempts, 0);
        assert_eq!(idle.last_error, None);
        Ok(())
    }

    #[test]
    fn reconnect_backoff_cancellation_and_teardown_are_stateful(
    ) -> Result<(), TransportSessionError> {
        let policy = ReconnectBackoffPolicy::new(100, 1_000, 2, 2)?;
        assert_eq!(policy.delay_for_attempt(1), 100);
        assert_eq!(policy.delay_for_attempt(2), 200);
        assert_eq!(policy.delay_for_attempt(8), 1_000);
        assert_eq!(policy.delay_for_attempt(u32::MAX), 1_000);
        assert!(ReconnectBackoffPolicy::new(0, 1_000, 2, 2).is_err());
        assert!(ReconnectBackoffPolicy::new(1_000, 100, 2, 2).is_err());
        assert!(ReconnectBackoffPolicy::new(100, 1_000, 0, 2).is_err());
        assert!(ReconnectBackoffPolicy::new(100, 1_000, 2, 0).is_err());

        let mut session = ready_session()?;
        session.select_direct(Endpoint::new("stun:direct.example:3478"))?;
        session.mark_disconnected("candidate pair failed")?;
        let first = session.schedule_reconnect(policy)?;
        assert_eq!(
            first,
            ReconnectDecision {
                attempt: 1,
                delay_ms: 100
            }
        );
        assert_eq!(session.state(), TransportSessionState::Reconnecting);
        session.begin_signaling()?;
        session.begin_ice_gathering()?;
        session.begin_checking()?;
        session.select_turn_relay(Endpoint::new("turns:relay.example:5349"))?;
        let torn_down = session.tear_down("user left voice channel")?;
        assert_eq!(torn_down.state, TransportSessionState::Cancelled);
        assert_eq!(torn_down.route, None);
        assert_eq!(
            torn_down.last_error.as_deref(),
            Some("user left voice channel")
        );
        let reset = session.reset()?;
        assert_eq!(reset.state, TransportSessionState::Idle);

        let mut exhausted = ready_session()?;
        exhausted.select_direct(Endpoint::new("stun:direct.example:3478"))?;
        exhausted.mark_disconnected("network changed")?;
        exhausted.schedule_reconnect(policy)?;
        assert_eq!(
            exhausted.schedule_reconnect(policy),
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Reconnecting,
                event: TransportSessionEvent::StartReconnecting,
            })
        );
        exhausted.begin_signaling()?;
        exhausted.begin_ice_gathering()?;
        exhausted.begin_checking()?;
        exhausted.select_direct(Endpoint::new("stun:direct.example:3478"))?;
        exhausted.mark_disconnected("network changed again")?;
        exhausted.schedule_reconnect(policy)?;
        exhausted.begin_signaling()?;
        exhausted.begin_ice_gathering()?;
        exhausted.begin_checking()?;
        exhausted.select_direct(Endpoint::new("stun:direct.example:3478"))?;
        exhausted.mark_disconnected("network changed final")?;
        assert_eq!(
            exhausted.schedule_reconnect(policy),
            Err(TransportSessionError::ReconnectAttemptsExhausted { attempts: 2 })
        );
        assert_eq!(exhausted.state(), TransportSessionState::Failed);

        let mut cancelled = ready_session()?;
        let cancelled_snapshot = cancelled.cancel("join flow cancelled")?;
        assert_eq!(cancelled_snapshot.state, TransportSessionState::Cancelled);
        assert_eq!(
            cancelled.cancel("again"),
            Err(TransportSessionError::InvalidTransition {
                from: TransportSessionState::Cancelled,
                event: TransportSessionEvent::Cancel,
            })
        );
        Ok(())
    }
}
