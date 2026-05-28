//! Transport trait for native QUIC now and web/DataChannel later.
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Transport address.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Endpoint(pub String);
#[derive(Debug, Error)]
pub enum TransportError {
    #[error("unavailable: {0}")]
    Unavailable(String),
}
#[async_trait]
pub trait Transport: Send + Sync {
    async fn send_datagram(&self, to: Endpoint, bytes: Vec<u8>) -> Result<(), TransportError>;
}
/// Phase-0 loopback transport for tests.
pub struct LoopbackTransport;
#[async_trait]
impl Transport for LoopbackTransport {
    async fn send_datagram(&self, _to: Endpoint, _bytes: Vec<u8>) -> Result<(), TransportError> {
        Ok(())
    }
}
