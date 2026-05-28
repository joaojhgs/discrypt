//! Content-free push wake abstraction.
use serde::{Deserialize, Serialize};
/// Push wake payload; intentionally opaque and content-free.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WakePayload {
    pub wake_token_hash: [u8; 32],
    pub reason: WakeReason,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WakeReason {
    IncomingCall,
    SyncHint,
}
#[must_use]
pub fn contains_content(_: &WakePayload) -> bool {
    false
}
