//! Abuse controls for invites, spam, and relay freeloading.
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Simple fixed-window limiter.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RateLimiter {
    limit: u32,
    window: Duration,
    hits: BTreeMap<String, Vec<DateTime<Utc>>>,
}
impl RateLimiter {
    #[must_use]
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            hits: BTreeMap::new(),
        }
    }
    pub fn allow(&mut self, key: &str, now: DateTime<Utc>) -> bool {
        let hits = self.hits.entry(key.into()).or_default();
        hits.retain(|t| *t + self.window >= now);
        if hits.len() as u32 >= self.limit {
            return false;
        }
        hits.push(now);
        true
    }
}
