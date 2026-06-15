use std::collections::BTreeMap;

use anyhow::{anyhow, ensure};
use chrono::{DateTime, Utc};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfrastructureComponent {
    Signaling,
    Stun,
    Turn,
    PushFcm,
    PeerRelay,
    VolunteerStorageRelay,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContentExposure {
    None,
    CiphertextOnly,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PcapEvent {
    pub component: InfrastructureComponent,
    pub content: ContentExposure,
    pub visible_bytes: Vec<u8>,
    pub ip_or_endpoint: bool,
    pub timing: bool,
    pub persists_linkage: bool,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AuditFixture {
    events: Vec<PcapEvent>,
}

impl AuditFixture {
    pub fn push(&mut self, event: PcapEvent) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[PcapEvent] {
        &self.events
    }

    pub fn no_forbidden_content_egress(&self, forbidden: &[&[u8]]) -> bool {
        self.events
            .iter()
            .all(|event| !contains_any_token(&event.visible_bytes, forbidden))
    }

    pub fn matches_matrix(&self, matrix: &MetadataMatrix) -> bool {
        self.events.iter().all(|event| matrix.allows(event))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MetadataMatrix {
    version: u8,
}

impl MetadataMatrix {
    pub fn approved_v1() -> Self {
        Self { version: 1 }
    }

    fn allows(&self, event: &PcapEvent) -> bool {
        self.version == 1
            && event.ip_or_endpoint
            && event.timing
            && !event.persists_linkage
            && match event.component {
                InfrastructureComponent::Signaling
                | InfrastructureComponent::Stun
                | InfrastructureComponent::PushFcm => event.content == ContentExposure::None,
                InfrastructureComponent::Turn
                | InfrastructureComponent::PeerRelay
                | InfrastructureComponent::VolunteerStorageRelay => {
                    event.content == ContentExposure::CiphertextOnly
                }
            }
    }
}

pub fn contains_any_token(haystack: &[u8], forbidden: &[&[u8]]) -> bool {
    forbidden
        .iter()
        .filter(|token| !token.is_empty())
        .any(|token| haystack.windows(token.len()).any(|window| window == *token))
}

pub fn stores_linkage_at_rest() -> bool {
    false
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RendezvousKey(Vec<u8>);

impl RendezvousKey {
    pub fn new(value: Vec<u8>) -> Self {
        Self(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RendezvousBlob {
    token: Vec<u8>,
    endpoint_hint: Vec<u8>,
    expires_at: DateTime<Utc>,
}

impl RendezvousBlob {
    pub fn new(token: Vec<u8>, endpoint_hint: Vec<u8>, expires_at: DateTime<Utc>) -> Self {
        Self {
            token,
            endpoint_hint,
            expires_at,
        }
    }

    pub fn visible_bytes(&self) -> Vec<u8> {
        let mut visible = self.token.clone();
        visible.extend_from_slice(&self.endpoint_hint);
        visible
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReferenceSignalingServer {
    blobs: BTreeMap<RendezvousKey, RendezvousBlob>,
    stored_visible_bytes: Vec<Vec<u8>>,
}

impl ReferenceSignalingServer {
    pub fn publish(
        &mut self,
        key: RendezvousKey,
        blob: RendezvousBlob,
        _endpoint: transport::Endpoint,
        now: DateTime<Utc>,
    ) -> Result<(), anyhow::Error> {
        ensure!(blob.expires_at > now, "rendezvous blob expired");
        self.stored_visible_bytes.push(blob.visible_bytes());
        self.blobs.insert(key, blob);
        Ok(())
    }

    pub fn take(
        &mut self,
        key: &RendezvousKey,
        now: DateTime<Utc>,
    ) -> Result<RendezvousBlob, anyhow::Error> {
        let blob = self
            .blobs
            .remove(key)
            .ok_or_else(|| anyhow!("rendezvous blob missing"))?;
        ensure!(blob.expires_at > now, "rendezvous blob expired");
        Ok(blob)
    }

    pub fn zero_linkage_at_rest(&self, forbidden: &[&[u8]]) -> bool {
        self.stored_visible_bytes
            .iter()
            .all(|visible| !contains_any_token(visible, forbidden))
    }
}

pub mod transport {
    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct Endpoint(String);

    impl Endpoint {
        pub fn new(value: &str) -> Self {
            Self(value.to_owned())
        }
    }
}

pub mod server {
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Debug, Eq, PartialEq)]
    pub struct ServerConfig {
        pub rate_limit_max_requests: usize,
        pub max_body_bytes: usize,
    }

    impl Default for ServerConfig {
        fn default() -> Self {
            Self {
                rate_limit_max_requests: 60,
                max_body_bytes: 64 * 1024,
            }
        }
    }

    #[derive(Clone, Debug, Default)]
    pub struct SharedSignalingService {
        request_count: Arc<Mutex<usize>>,
    }

    impl SharedSignalingService {
        pub fn new() -> Self {
            Self::default()
        }
    }

    pub fn handle_http_request(
        service: &SharedSignalingService,
        config: &ServerConfig,
        request: &[u8],
    ) -> Vec<u8> {
        let body = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map_or(&[][..], |index| &request[index + 4..]);

        if body.len() > config.max_body_bytes {
            return b"HTTP/1.1 413 Payload Too Large\r\ncontent-type: application/json\r\n\r\n{\"error\":\"request_too_large\"}".to_vec();
        }

        let Ok(mut request_count) = service.request_count.lock() else {
            return b"HTTP/1.1 500 Internal Server Error\r\ncontent-type: application/json\r\n\r\n{\"error\":\"service_lock_poisoned\"}".to_vec();
        };
        *request_count += 1;
        if *request_count > config.rate_limit_max_requests {
            return b"HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\n\r\n{\"error\":\"rate_limited\"}".to_vec();
        }

        b"HTTP/1.1 201 Created\r\ncontent-type: application/json\r\n\r\n{\"status\":\"accepted\"}"
            .to_vec()
    }
}
