//! Typed transport-session state machine for ICE/WebRTC route negotiation.
//!
//! This module models the production state boundary without parsing ICE
//! configuration or generating WebRTC offers/candidates. Later transport work can
//! attach concrete negotiation data to these typed states and events.

use crate::{Endpoint, FallbackLeg};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Production transport session state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportSessionState {
    /// No active signaling or transport path exists.
    Idle,
    /// Signaling/rendezvous exchange is active.
    Signaling,
    /// Local ICE candidate gathering is active.
    IceGathering,
    /// Candidate pairs are being checked.
    Checking,
    /// Direct ICE path is selected.
    Direct,
    /// Peer overlay relay path is selected.
    OverlayRelay,
    /// TURN relay path is selected.
    TurnRelay,
    /// Previously connected path is down.
    Disconnected,
    /// Reconnect/backoff loop is active.
    Reconnecting,
    /// Session failed terminally until reset.
    Failed,
}

impl TransportSessionState {
    /// True when the state has an active transport path.
    #[must_use]
    pub const fn connected(self) -> bool {
        matches!(self, Self::Direct | Self::OverlayRelay | Self::TurnRelay)
    }
}

/// Typed event applied to a [`TransportSession`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransportSessionEvent {
    /// Begin signaling/rendezvous.
    StartSignaling,
    /// Local ICE gathering started.
    IceGatheringStarted,
    /// ICE candidate-pair checks started.
    CheckingStarted,
    /// A direct path was selected.
    DirectSelected { endpoint: Endpoint },
    /// An overlay relay path was selected.
    OverlayRelaySelected { endpoint: Endpoint },
    /// A TURN relay path was selected.
    TurnRelaySelected { endpoint: Endpoint },
    /// The active path disconnected.
    ConnectionLost,
    /// Reconnect/backoff loop started.
    ReconnectStarted,
    /// Reconnect loop is retrying signaling.
    RetrySignaling,
    /// Terminal failure.
    Fail { reason: String },
    /// Reset session to idle.
    Reset,
}

/// Invalid state transition.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
#[error("invalid transport session transition from {from:?} with {event:?}")]
pub struct TransportSessionTransitionError {
    /// State before the rejected event.
    pub from: TransportSessionState,
    /// Rejected event.
    pub event: TransportSessionEvent,
}

/// Successful typed transport session transition.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportSessionTransition {
    /// State before the event was applied.
    pub from: TransportSessionState,
    /// Event that was accepted.
    pub event: TransportSessionEvent,
    /// State after the event was applied.
    pub to: TransportSessionState,
    /// Serializable session snapshot after the event was applied.
    pub snapshot: TransportSessionSnapshot,
}

/// State-machine backing one peer transport negotiation/session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportSession {
    state: TransportSessionState,
    selected_leg: Option<FallbackLeg>,
    endpoint: Option<Endpoint>,
    reconnect_attempts: u32,
    last_error: Option<String>,
}

impl Default for TransportSession {
    fn default() -> Self {
        Self::new()
    }
}

impl TransportSession {
    /// Construct an idle session.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            state: TransportSessionState::Idle,
            selected_leg: None,
            endpoint: None,
            reconnect_attempts: 0,
            last_error: None,
        }
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> TransportSessionState {
        self.state
    }

    /// Return true when this event is valid from the current state.
    #[must_use]
    pub fn can_apply(&self, event: &TransportSessionEvent) -> bool {
        self.next_state(event).is_some()
    }

    /// Apply one typed event and return the resulting status snapshot.
    pub fn apply(
        &mut self,
        event: TransportSessionEvent,
    ) -> Result<TransportSessionSnapshot, TransportSessionTransitionError> {
        Ok(self.transition(event)?.snapshot)
    }

    /// Apply one typed event and return the full transition record.
    pub fn transition(
        &mut self,
        event: TransportSessionEvent,
    ) -> Result<TransportSessionTransition, TransportSessionTransitionError> {
        let from = self.state;
        let next = self
            .next_state(&event)
            .ok_or_else(|| TransportSessionTransitionError {
                from,
                event: event.clone(),
            })?;
        self.commit(next, event.clone());
        Ok(TransportSessionTransition {
            from,
            event,
            to: next,
            snapshot: self.snapshot(),
        })
    }

    /// Serializable snapshot for Tauri/UI status surfaces.
    #[must_use]
    pub fn snapshot(&self) -> TransportSessionSnapshot {
        TransportSessionSnapshot {
            state: self.state,
            connected: self.state.connected(),
            selected_leg: self.selected_leg,
            endpoint: self.endpoint.clone(),
            reconnect_attempts: self.reconnect_attempts,
            last_error: self.last_error.clone(),
        }
    }

    fn next_state(&self, event: &TransportSessionEvent) -> Option<TransportSessionState> {
        use TransportSessionEvent as Event;
        use TransportSessionState as State;
        match (self.state, event) {
            (_, Event::Reset) => Some(State::Idle),
            (_, Event::Fail { .. }) => Some(State::Failed),
            (State::Idle, Event::StartSignaling) => Some(State::Signaling),
            (State::Signaling, Event::IceGatheringStarted) => Some(State::IceGathering),
            (State::IceGathering, Event::CheckingStarted) => Some(State::Checking),
            (State::Checking, Event::DirectSelected { .. }) => Some(State::Direct),
            (State::Checking, Event::OverlayRelaySelected { .. }) => Some(State::OverlayRelay),
            (State::Checking, Event::TurnRelaySelected { .. }) => Some(State::TurnRelay),
            (State::Direct | State::OverlayRelay | State::TurnRelay, Event::ConnectionLost) => {
                Some(State::Disconnected)
            }
            (State::Disconnected, Event::ReconnectStarted) => Some(State::Reconnecting),
            (State::Reconnecting, Event::RetrySignaling) => Some(State::Signaling),
            _ => None,
        }
    }

    fn commit(&mut self, next: TransportSessionState, event: TransportSessionEvent) {
        use TransportSessionEvent as Event;
        self.state = next;
        match event {
            Event::DirectSelected { endpoint } => {
                self.selected_leg = Some(FallbackLeg::Stun);
                self.endpoint = Some(endpoint);
                self.last_error = None;
            }
            Event::OverlayRelaySelected { endpoint } => {
                self.selected_leg = Some(FallbackLeg::RelayOverlay);
                self.endpoint = Some(endpoint);
                self.last_error = None;
            }
            Event::TurnRelaySelected { endpoint } => {
                self.selected_leg = Some(FallbackLeg::Turn);
                self.endpoint = Some(endpoint);
                self.last_error = None;
            }
            Event::ConnectionLost => {
                self.last_error = Some("connection_lost".to_owned());
            }
            Event::ReconnectStarted => {
                self.reconnect_attempts = self.reconnect_attempts.saturating_add(1);
            }
            Event::RetrySignaling => {
                self.selected_leg = None;
                self.endpoint = None;
            }
            Event::Fail { reason } => {
                self.last_error = Some(reason);
            }
            Event::Reset => {
                self.selected_leg = None;
                self.endpoint = None;
                self.reconnect_attempts = 0;
                self.last_error = None;
            }
            Event::StartSignaling | Event::IceGatheringStarted | Event::CheckingStarted => {
                self.last_error = None;
            }
        }
    }
}

/// Serializable transport session status for app-service/Tauri/UI boundaries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TransportSessionSnapshot {
    /// Current state.
    pub state: TransportSessionState,
    /// True when direct, overlay, or TURN path is active.
    pub connected: bool,
    /// Selected fallback leg once connected.
    pub selected_leg: Option<FallbackLeg>,
    /// Selected endpoint or route descriptor once connected.
    pub endpoint: Option<Endpoint>,
    /// Number of reconnect cycles started.
    pub reconnect_attempts: u32,
    /// Last failure/disconnect reason, if any.
    pub last_error: Option<String>,
}
