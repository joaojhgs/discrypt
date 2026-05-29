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
    /// The encrypted peer overlay relay route is active.
    OverlayRelay,
    /// The encrypted TURN route is active.
    TurnRelay,
    /// A previously active route was lost.
    Disconnected,
    /// Reconnection orchestration is in progress.
    Reconnecting,
    /// The session has reached a terminal failure until reset.
    Failed,
}

impl TransportSessionState {
    /// The exact state set promised by the G041 transport-session contract.
    pub const ALL: [Self; 10] = [
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
    ];

    /// True when the state represents an active data route.
    #[must_use]
    pub const fn is_connected(self) -> bool {
        matches!(self, Self::Direct | Self::OverlayRelay | Self::TurnRelay)
    }
}

/// Route kind selected by the transport session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TransportRoute {
    /// Direct ICE/STUN connectivity.
    Direct,
    /// Encrypted peer overlay relay fallback.
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
    /// Select encrypted peer overlay relay connectivity.
    SelectOverlayRelay,
    /// Select encrypted TURN connectivity.
    SelectTurnRelay,
    /// Mark an active route disconnected.
    MarkDisconnected,
    /// Begin reconnection after disconnection.
    StartReconnecting,
    /// Mark the session failed.
    MarkFailed,
    /// Reset a failed or disconnected session back to idle.
    Reset,
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

    /// Select an encrypted overlay relay route from the checking state.
    pub fn select_overlay_relay(
        &mut self,
        endpoint: Endpoint,
    ) -> Result<TransportSessionSnapshot, TransportSessionError> {
        self.select_route(TransportRoute::OverlayRelay, endpoint, None)
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
    fn overlay_and_turn_paths_follow_planner_selection() -> Result<(), Box<dyn std::error::Error>> {
        let config = ConnectivityConfig::default();

        let mut overlay = ready_session()?;
        let overlay_plan = ConnectivityPlanner::plan(&config, SimulatedNat::overlay_only())?;
        let overlay_snapshot = overlay.select_connectivity_plan(overlay_plan)?;
        assert_eq!(overlay_snapshot.state, TransportSessionState::OverlayRelay);
        assert_eq!(
            overlay_snapshot.route.as_ref().map(|route| (
                route.route,
                route.route_report.as_ref().map(|report| report.selected)
            )),
            Some((
                TransportRoute::OverlayRelay,
                Some(FallbackLeg::RelayOverlay)
            ))
        );

        let mut turn = ready_session()?;
        let turn_plan = ConnectivityPlanner::plan(&config, SimulatedNat::turn_only())?;
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
}
