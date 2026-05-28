//! Content-key retention, live-key, and shred primitives.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

/// Retention window presets.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RetentionWindow {
    Hours1,
    Hours24,
    Days7,
    Days30,
    Days90,
    CustomSeconds(u64),
    UnlimitedWarned,
}
impl RetentionWindow {
    #[must_use]
    pub fn seconds(self) -> Option<u64> {
        match self {
            Self::Hours1 => Some(3600),
            Self::Hours24 => Some(86400),
            Self::Days7 => Some(604800),
            Self::Days30 => Some(2592000),
            Self::Days90 => Some(7776000),
            Self::CustomSeconds(s) => Some(s),
            Self::UnlimitedWarned => None,
        }
    }
}

/// Cached, locked, or shredded message-key state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum KeyState {
    Cached([u8; 32]),
    Locked,
    Shredded,
}

/// Deterministic content key derivation for tests/facade.
#[must_use]
pub fn derive_content_key(author: u32, message_id: &str, epoch_secret: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"discrypt-content-key");
    h.update(author.to_be_bytes());
    h.update(message_id.as_bytes());
    h.update(epoch_secret);
    h.finalize().into()
}

/// Apply retention to message timestamp.
#[must_use]
pub fn key_state(
    now: DateTime<Utc>,
    sent_at: DateTime<Utc>,
    window: RetentionWindow,
    key: [u8; 32],
    tombstoned: bool,
) -> KeyState {
    if tombstoned {
        return KeyState::Shredded;
    }
    match window.seconds() {
        None => KeyState::Cached(key),
        Some(s) if now.signed_duration_since(sent_at) <= Duration::seconds(s as i64) => {
            KeyState::Cached(key)
        }
        Some(_) => KeyState::Locked,
    }
}

/// Tombstone set.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Tombstones {
    ids: BTreeSet<String>,
}
impl Tombstones {
    pub fn shred(&mut self, id: impl Into<String>) {
        self.ids.insert(id.into());
    }
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shorten_locks_old_messages() {
        let now = Utc::now();
        let key = [3; 32];
        assert_eq!(
            key_state(
                now,
                now - Duration::hours(2),
                RetentionWindow::Hours1,
                key,
                false
            ),
            KeyState::Locked
        );
        assert!(matches!(
            key_state(
                now,
                now - Duration::hours(2),
                RetentionWindow::UnlimitedWarned,
                key,
                false
            ),
            KeyState::Cached(_)
        ));
    }
}
