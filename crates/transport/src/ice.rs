//! ICE/STUN/TURN endpoint policy parsing and deterministic fallback conversion.
//!
//! These types intentionally stop at typed configuration and validation. They do
//! not create WebRTC offers, gather candidates, or open media transports.

use crate::{ConnectivityConfig, Endpoint, EndpointOverrides, TransportError};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha1::Sha1;

type HmacSha1 = Hmac<Sha1>;

/// ADR-004 decision record for STUN/TURN credential and policy handling.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnStunCredentialDecision {
    /// Default STUN endpoint class used when an invite does not carry an override.
    pub default_stun_endpoint: &'static str,
    /// Default TURN endpoint class used when an invite does not carry an override.
    pub default_turn_endpoint: &'static str,
    /// Group ICE policy precedence over invite ICE policy.
    pub group_override_rule: &'static str,
    /// Ephemeral credential mechanism used by the self-hosting TURN service.
    pub ephemeral_credential_service: &'static str,
    /// Invite descriptor field that signs STUN/TURN references.
    pub invite_descriptor_reference: &'static str,
    /// Rotation policy for TURN credentials.
    pub credential_rotation: &'static str,
    /// Self-hosting artifacts operators must configure.
    pub self_hosting_docs: &'static str,
}

impl TurnStunCredentialDecision {
    /// Return true when the code-level decision covers every ADR-004 axis.
    #[must_use]
    pub fn covers_adr_004(&self) -> bool {
        self.default_stun_endpoint.contains("stun:")
            && self.default_turn_endpoint.contains("turns:")
            && self.group_override_rule.contains("group")
            && self.group_override_rule.contains("invite")
            && self.ephemeral_credential_service.contains("TURN service REST")
            && self.ephemeral_credential_service.contains("HMAC-SHA1")
            && self
                .invite_descriptor_reference
                .contains("ice_endpoint_policy")
            && self.credential_rotation.contains("expires_at")
            && self.credential_rotation.contains("reject expired")
            && self
                .self_hosting_docs
                .contains("EXTERNAL_TURN_STATIC_AUTH_SECRET")
            && self.self_hosting_docs.contains("TURN service")
    }
}

/// Current ADR-004 TURN/STUN credential decision.
#[must_use]
pub fn turn_stun_credential_decision() -> TurnStunCredentialDecision {
    TurnStunCredentialDecision {
        default_stun_endpoint: "stun:default.discrypt.invalid:3478",
        default_turn_endpoint: "turns:default.discrypt.invalid:5349",
        group_override_rule: "non-empty group IceEndpointPolicy stun_servers/turn_servers override signed invite policy by endpoint kind; otherwise invite policy falls back to defaults",
        ephemeral_credential_service: "TURN service REST auth credentials: username is unix-expiry:subject, credential is base64(HMAC-SHA1(static_auth_secret, username)), carried only as ephemeral TurnServerConfig material",
        invite_descriptor_reference: "InviteSignalingMetadata signs ice_endpoint_policy into StoredInvite signing bytes so invite descriptors reference signaling, STUN, TURN, expiry, and trust metadata together",
        credential_rotation: "TURN credentials carry credential_expires_at; clients reject expired or incomplete credentials before WebRTC offer generation and operators rotate by issuing a fresh invite/group policy",
        self_hosting_docs: "external/signaling-repository/deploy/external-signaling.env.example, TURN service.conf.example, compose.yml, and external-signaling-operations.md document EXTERNAL_TURN_STATIC_AUTH_SECRET and TURN service self-hosting",
    }
}

/// Signed invite/group policy containing ICE server endpoints joiners may use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IceEndpointPolicy {
    /// STUN endpoints used for direct ICE candidate discovery.
    #[serde(default)]
    pub stun_servers: Vec<Endpoint>,
    /// TURN endpoints used as the final relay fallback.
    #[serde(default)]
    pub turn_servers: Vec<TurnServerConfig>,
}

impl Default for IceEndpointPolicy {
    fn default() -> Self {
        Self::default_production()
    }
}

impl IceEndpointPolicy {
    /// Deterministic safe defaults used when an invite carries no custom ICE policy.
    #[must_use]
    pub fn default_production() -> Self {
        Self {
            stun_servers: vec![Endpoint::new("stun:default.discrypt.invalid:3478")],
            turn_servers: vec![TurnServerConfig::new(
                Endpoint::new("turns:default.discrypt.invalid:5349"),
                None,
                None,
                None,
            )],
        }
    }

    /// Construct a policy from explicit STUN and TURN metadata after endpoint validation.
    pub fn new(
        stun_servers: Vec<Endpoint>,
        turn_servers: Vec<TurnServerConfig>,
    ) -> Result<Self, TransportError> {
        let policy = Self {
            stun_servers,
            turn_servers,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Validate endpoint schemes and required policy shape.
    pub fn validate(&self) -> Result<(), TransportError> {
        if self.stun_servers.is_empty() && self.turn_servers.is_empty() {
            return Err(TransportError::InvalidIcePolicy(
                "at least one STUN or TURN server is required".to_owned(),
            ));
        }
        for endpoint in &self.stun_servers {
            validate_endpoint(endpoint, EndpointKind::Stun)?;
        }
        for server in &self.turn_servers {
            server.validate()?;
        }
        Ok(())
    }

    /// Validate endpoint schemes plus any time-limited TURN credentials at `now`.
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), TransportError> {
        self.validate()?;
        for server in &self.turn_servers {
            server.validate_credentials_at(now)?;
        }
        Ok(())
    }

    /// Resolve typed ICE config, with non-empty group fields overriding invite fields.
    pub fn resolve(
        invite_policy: Option<&Self>,
        group_policy: Option<&Self>,
    ) -> Result<IceServerConfig, TransportError> {
        let default_policy;
        let invite = if let Some(policy) = invite_policy {
            policy
        } else {
            default_policy = Self::default_production();
            &default_policy
        };
        invite.validate()?;
        if let Some(group) = group_policy {
            group.validate()?;
        }

        let stun_servers = group_policy
            .filter(|policy| !policy.stun_servers.is_empty())
            .map(|policy| policy.stun_servers.clone())
            .unwrap_or_else(|| invite.stun_servers.clone());
        let turn_servers = group_policy
            .filter(|policy| !policy.turn_servers.is_empty())
            .map(|policy| policy.turn_servers.clone())
            .unwrap_or_else(|| invite.turn_servers.clone());

        IceServerConfig::new(stun_servers, turn_servers)
    }

    /// Resolve typed ICE config and reject expired or incomplete TURN credentials.
    pub fn resolve_at(
        invite_policy: Option<&Self>,
        group_policy: Option<&Self>,
        now: DateTime<Utc>,
    ) -> Result<IceServerConfig, TransportError> {
        let config = Self::resolve(invite_policy, group_policy)?;
        config.validate_credentials_at(now)?;
        Ok(config)
    }

    /// Canonical bytes included in signed invite descriptors.
    #[must_use]
    pub fn signing_bytes(&self) -> Vec<u8> {
        let mut bytes = b"discrypt-ice-endpoint-policy-v1".to_vec();
        bytes.extend_from_slice(&(self.stun_servers.len() as u64).to_le_bytes());
        for endpoint in &self.stun_servers {
            push_string(&mut bytes, &endpoint.0);
        }
        bytes.extend_from_slice(&(self.turn_servers.len() as u64).to_le_bytes());
        for server in &self.turn_servers {
            push_string(&mut bytes, &server.endpoint.0);
            push_optional_string(&mut bytes, server.username.as_deref());
            push_optional_string(&mut bytes, server.credential.as_deref());
            push_optional_string(&mut bytes, server.credential_expires_at.as_deref());
        }
        bytes
    }
}

/// Validated ICE server config consumed by transport planning and future WebRTC setup.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct IceServerConfig {
    /// STUN endpoints after invite/group precedence resolution.
    pub stun_servers: Vec<Endpoint>,
    /// TURN endpoints after invite/group precedence resolution.
    pub turn_servers: Vec<TurnServerConfig>,
}

impl IceServerConfig {
    /// Construct a typed config from already parsed endpoint metadata.
    pub fn new(
        stun_servers: Vec<Endpoint>,
        turn_servers: Vec<TurnServerConfig>,
    ) -> Result<Self, TransportError> {
        let policy = IceEndpointPolicy::new(stun_servers, turn_servers)?;
        Ok(Self {
            stun_servers: policy.stun_servers,
            turn_servers: policy.turn_servers,
        })
    }

    /// Validate time-limited TURN credentials at `now`.
    pub fn validate_credentials_at(&self, now: DateTime<Utc>) -> Result<(), TransportError> {
        for server in &self.turn_servers {
            server.validate_credentials_at(now)?;
        }
        Ok(())
    }

    /// Convert the first STUN/TURN choices into the existing fallback planner config after credential validation.
    pub fn to_connectivity_config_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<ConnectivityConfig, TransportError> {
        self.validate_credentials_at(now)?;
        Ok(self.to_connectivity_config())
    }

    /// Convert the first STUN/TURN choices into the existing fallback planner config.
    #[must_use]
    pub fn to_connectivity_config(&self) -> ConnectivityConfig {
        let defaults = ConnectivityConfig::default();
        ConnectivityConfig {
            overrides: EndpointOverrides::new(
                self.stun_servers.first().cloned(),
                self.turn_servers
                    .first()
                    .map(|turn_server| turn_server.endpoint.clone()),
            ),
            ..defaults
        }
    }
}

/// Redacted TURN credential shape selected for a provider.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnCredentialMode {
    /// No TURN authentication material is configured.
    Unauthenticated,
    /// Long-lived or externally rotated configured credentials are present.
    Configured,
    /// Short-lived TURN credentials are present and valid until this timestamp.
    Ephemeral {
        /// RFC3339 expiration time for the ephemeral credential.
        expires_at: String,
    },
}

/// Configuration for issuing short-lived TURN service REST-auth credentials.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnCredentialIssuerConfig {
    /// Public TURN or TURNS endpoint to embed into invite/group ICE policy.
    pub public_turn_endpoint: Endpoint,
    /// TURN realm configured in TURN service.
    pub realm: String,
    /// Coturn REST static-auth-secret bytes. Never place this value in invites.
    #[serde(skip_serializing)]
    pub static_auth_secret: Vec<u8>,
    /// Credential lifetime in seconds.
    pub ttl_seconds: u32,
    /// Optional subject prefix for usernames.
    #[serde(default)]
    pub username_prefix: Option<String>,
}

impl TurnCredentialIssuerConfig {
    /// Construct and validate an ephemeral TURN credential issuer.
    pub fn new(
        public_turn_endpoint: Endpoint,
        realm: impl Into<String>,
        static_auth_secret: Vec<u8>,
        ttl_seconds: u32,
        username_prefix: Option<String>,
    ) -> Result<Self, TransportError> {
        let config = Self {
            public_turn_endpoint,
            realm: realm.into(),
            static_auth_secret,
            ttl_seconds,
            username_prefix,
        };
        config.validate()?;
        Ok(config)
    }

    /// Validate TURN service REST-auth issuer configuration before it is used.
    pub fn validate(&self) -> Result<(), TransportError> {
        validate_endpoint(&self.public_turn_endpoint, EndpointKind::Turn)?;
        if self.realm.trim().is_empty()
            || self.realm.trim() != self.realm
            || self.realm.chars().any(char::is_whitespace)
        {
            return Err(TransportError::InvalidIcePolicy(
                "TURN realm must be non-empty, trimmed, and whitespace-free".to_owned(),
            ));
        }
        if self.static_auth_secret.len() < 32 {
            return Err(TransportError::InvalidIcePolicy(
                "TURN static auth secret must be at least 32 bytes".to_owned(),
            ));
        }
        if !(60..=86_400).contains(&self.ttl_seconds) {
            return Err(TransportError::InvalidIcePolicy(
                "TURN credential TTL must be between 60 and 86400 seconds".to_owned(),
            ));
        }
        if let Some(prefix) = &self.username_prefix {
            if prefix.trim() != prefix || prefix.chars().any(char::is_whitespace) {
                return Err(TransportError::InvalidIcePolicy(
                    "TURN username prefix must be trimmed and whitespace-free".to_owned(),
                ));
            }
        }
        Ok(())
    }
}

/// Stateless TURN service REST-auth credential issuer used by invite/group policy code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TurnCredentialIssuer {
    config: TurnCredentialIssuerConfig,
}

impl TurnCredentialIssuer {
    /// Construct a credential issuer from validated self-hosting configuration.
    pub fn new(config: TurnCredentialIssuerConfig) -> Result<Self, TransportError> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Issue one short-lived TURN credential for an opaque subject.
    pub fn issue_for_subject(
        &self,
        subject: &str,
        now: DateTime<Utc>,
    ) -> Result<TurnServerConfig, TransportError> {
        let subject = sanitized_turn_subject(subject)?;
        let expires_at = now + chrono::Duration::seconds(i64::from(self.config.ttl_seconds));
        let expiry = expires_at.timestamp();
        let username = match &self.config.username_prefix {
            Some(prefix) if !prefix.is_empty() => format!("{expiry}:{prefix}:{subject}"),
            _ => format!("{expiry}:{subject}"),
        };
        let mut mac = HmacSha1::new_from_slice(&self.config.static_auth_secret).map_err(|_| {
            TransportError::InvalidIcePolicy("TURN static auth secret invalid".to_owned())
        })?;
        mac.update(username.as_bytes());
        let credential = BASE64_STANDARD.encode(mac.finalize().into_bytes());
        Ok(TurnServerConfig::new(
            self.config.public_turn_endpoint.clone(),
            Some(username),
            Some(credential),
            Some(expires_at.to_rfc3339()),
        ))
    }
}

/// TURN server endpoint plus optional configured or short-lived credentials from invite/group policy.
#[derive(Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnServerConfig {
    /// TURN or TURNS provider URI.
    pub endpoint: Endpoint,
    /// Username for ephemeral TURN credentials, if required by the server.
    #[serde(default)]
    pub username: Option<String>,
    /// Raw ephemeral TURN credential. Redacted from `Debug` output.
    #[serde(default)]
    pub credential: Option<String>,
    /// RFC3339 expiration time for the TURN credential, if present.
    #[serde(default)]
    pub credential_expires_at: Option<String>,
}

impl std::fmt::Debug for TurnServerConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TurnServerConfig")
            .field("endpoint", &self.endpoint)
            .field("username", &self.username)
            .field(
                "credential",
                &self.credential.as_ref().map(|_| "<redacted>"),
            )
            .field("credential_expires_at", &self.credential_expires_at)
            .finish()
    }
}

impl TurnServerConfig {
    /// Construct TURN server metadata. Expiry is parsed in the credential validation phase.
    #[must_use]
    pub fn new(
        endpoint: Endpoint,
        username: Option<String>,
        credential: Option<String>,
        credential_expires_at: Option<String>,
    ) -> Self {
        Self {
            endpoint,
            username,
            credential,
            credential_expires_at,
        }
    }

    /// Validate endpoint scheme and credential field pairing.
    pub fn validate(&self) -> Result<(), TransportError> {
        validate_endpoint(&self.endpoint, EndpointKind::Turn)?;
        self.validate_credential_shape()
    }

    /// Parse the TURN credential expiry timestamp when a time-limited credential is present.
    pub fn parsed_credential_expiry(&self) -> Result<Option<DateTime<Utc>>, TransportError> {
        self.credential_expires_at
            .as_deref()
            .map(|value| {
                DateTime::parse_from_rfc3339(value)
                    .map(|expires_at| expires_at.with_timezone(&Utc))
                    .map_err(|_| {
                        TransportError::InvalidIcePolicy(
                            "TURN credential expiry must be RFC3339".to_owned(),
                        )
                    })
            })
            .transpose()
    }

    /// Classify TURN credentials without exposing credential material.
    pub fn credential_mode_at(
        &self,
        now: DateTime<Utc>,
    ) -> Result<TurnCredentialMode, TransportError> {
        self.validate()?;
        if !self.credentials_declared() {
            return Ok(TurnCredentialMode::Unauthenticated);
        }
        if let Some(expires_at) = self.parsed_credential_expiry()? {
            if expires_at <= now {
                return Err(TransportError::InvalidIcePolicy(
                    "TURN credential expired".to_owned(),
                ));
            }
            return Ok(TurnCredentialMode::Ephemeral {
                expires_at: expires_at.to_rfc3339(),
            });
        }
        Ok(TurnCredentialMode::Configured)
    }

    /// Validate configured or time-limited TURN credentials and reject expired or incomplete policy.
    pub fn validate_credentials_at(&self, now: DateTime<Utc>) -> Result<(), TransportError> {
        self.credential_mode_at(now)?;
        Ok(())
    }

    fn credentials_declared(&self) -> bool {
        self.username.is_some() || self.credential.is_some() || self.credential_expires_at.is_some()
    }

    fn validate_credential_shape(&self) -> Result<(), TransportError> {
        if !self.credentials_declared() {
            return Ok(());
        }
        match (&self.username, &self.credential) {
            (Some(username), Some(credential))
                if !username.trim().is_empty() && !credential.trim().is_empty() =>
            {
                Ok(())
            }
            _ => Err(TransportError::InvalidIcePolicy(
                "TURN username and credential must be provided together".to_owned(),
            )),
        }
    }
}

#[derive(Clone, Copy)]
enum EndpointKind {
    Stun,
    Turn,
}

fn validate_endpoint(endpoint: &Endpoint, kind: EndpointKind) -> Result<(), TransportError> {
    let value = endpoint.0.as_str();
    if value.is_empty()
        || value.trim() != value
        || value.len() > 512
        || value.chars().any(char::is_whitespace)
    {
        return Err(TransportError::InvalidIcePolicy(
            "ICE endpoint must be non-empty, trimmed, and whitespace-free".to_owned(),
        ));
    }
    let valid_scheme = match kind {
        EndpointKind::Stun => value.starts_with("stun:") || value.starts_with("stuns:"),
        EndpointKind::Turn => value.starts_with("turn:") || value.starts_with("turns:"),
    };
    if valid_scheme {
        Ok(())
    } else {
        Err(TransportError::InvalidIcePolicy(format!(
            "unsupported ICE endpoint scheme: {value}"
        )))
    }
}

fn sanitized_turn_subject(subject: &str) -> Result<String, TransportError> {
    let trimmed = subject.trim();
    if trimmed.is_empty() {
        return Err(TransportError::InvalidIcePolicy(
            "TURN credential subject is required".to_owned(),
        ));
    }
    if trimmed.len() > 96 {
        return Err(TransportError::InvalidIcePolicy(
            "TURN credential subject is too long".to_owned(),
        ));
    }
    if !trimmed
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.'))
    {
        return Err(TransportError::InvalidIcePolicy(
            "TURN credential subject must be opaque ASCII token characters".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

fn push_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}

fn push_optional_string(bytes: &mut Vec<u8>, value: Option<&str>) {
    match value {
        Some(value) => {
            bytes.push(1);
            push_string(bytes, value);
        }
        None => bytes.push(0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ConnectivityPlanner, FallbackLeg, SimulatedNat};
    use chrono::Duration;

    #[test]
    fn parses_valid_stun_and_turn_policy_into_typed_config() -> Result<(), TransportError> {
        let policy = IceEndpointPolicy::new(
            vec![Endpoint::new("stun:invite.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("turns:invite.example.invalid:5349"),
                Some("joiner".to_owned()),
                Some("ephemeral-secret".to_owned()),
                Some("2026-05-29T17:00:00Z".to_owned()),
            )],
        )?;

        let config = IceEndpointPolicy::resolve(Some(&policy), None)?;

        assert_eq!(
            config.stun_servers,
            vec![Endpoint::new("stun:invite.example.invalid:3478")]
        );
        assert_eq!(
            config.turn_servers[0].endpoint,
            Endpoint::new("turns:invite.example.invalid:5349")
        );
        assert!(!format!("{:?}", config.turn_servers[0]).contains("ephemeral-secret"));
        Ok(())
    }

    #[test]
    fn rejects_invalid_ice_endpoint_schemes() {
        assert_eq!(
            IceEndpointPolicy::new(vec![Endpoint::new("https://stun.example.invalid")], vec![])
                .err(),
            Some(TransportError::InvalidIcePolicy(
                "unsupported ICE endpoint scheme: https://stun.example.invalid".to_owned()
            ))
        );
        assert!(IceEndpointPolicy::new(
            vec![Endpoint::new("stun:valid.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("https://turn.example.invalid"),
                None,
                None,
                None,
            )],
        )
        .is_err());
    }

    #[test]
    fn group_policy_overrides_invite_policy_by_endpoint_kind() -> Result<(), TransportError> {
        let invite = IceEndpointPolicy::new(
            vec![Endpoint::new("stun:invite.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("turns:invite.example.invalid:5349"),
                None,
                None,
                None,
            )],
        )?;
        let group = IceEndpointPolicy::new(
            vec![Endpoint::new("stuns:group.example.invalid:5349")],
            vec![TurnServerConfig::new(
                Endpoint::new("turn:group.example.invalid:3478"),
                None,
                None,
                None,
            )],
        )?;

        let config = IceEndpointPolicy::resolve(Some(&invite), Some(&group))?;
        let fallback = config.to_connectivity_config();
        let stun = ConnectivityPlanner::plan(&fallback, SimulatedNat::direct())?;
        let turn = ConnectivityPlanner::plan(&fallback, SimulatedNat::turn_only())?;

        assert_eq!(
            stun.endpoint,
            Endpoint::new("stuns:group.example.invalid:5349")
        );
        assert_eq!(turn.selected, FallbackLeg::Turn);
        assert_eq!(
            turn.endpoint,
            Endpoint::new("turn:group.example.invalid:3478")
        );
        Ok(())
    }

    #[test]
    fn conversion_rejects_missing_or_expired_turn_credentials() -> Result<(), TransportError> {
        let now = Utc::now();
        let missing_credential = IceServerConfig::new(
            vec![Endpoint::new("stun:valid.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("turns:turn.example.invalid:5349"),
                Some("joiner".to_owned()),
                None,
                Some((now + Duration::minutes(5)).to_rfc3339()),
            )],
        );
        assert_eq!(
            missing_credential.err(),
            Some(TransportError::InvalidIcePolicy(
                "TURN username and credential must be provided together".to_owned()
            ))
        );

        let expired = IceServerConfig::new(
            vec![Endpoint::new("stun:valid.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("turns:turn.example.invalid:5349"),
                Some("joiner".to_owned()),
                Some("secret".to_owned()),
                Some((now - Duration::minutes(1)).to_rfc3339()),
            )],
        )?;
        assert_eq!(
            expired.to_connectivity_config_at(now).err(),
            Some(TransportError::InvalidIcePolicy(
                "TURN credential expired".to_owned()
            ))
        );
        Ok(())
    }

    #[test]
    fn unexpired_turn_credentials_convert_to_existing_endpoint_overrides(
    ) -> Result<(), TransportError> {
        let now = Utc::now();
        let config = IceServerConfig::new(
            vec![Endpoint::new("stun:valid.example.invalid:3478")],
            vec![TurnServerConfig::new(
                Endpoint::new("turns:turn.example.invalid:5349"),
                Some("joiner".to_owned()),
                Some("secret".to_owned()),
                Some((now + Duration::minutes(5)).to_rfc3339()),
            )],
        )?;

        let connectivity = config.to_connectivity_config_at(now)?;
        let plan = ConnectivityPlanner::plan(&connectivity, SimulatedNat::turn_only())?;

        assert_eq!(plan.selected, FallbackLeg::Turn);
        assert_eq!(
            plan.endpoint,
            Endpoint::new("turns:turn.example.invalid:5349")
        );
        assert_eq!(connectivity.turn_endpoint(), plan.endpoint);
        Ok(())
    }

    #[test]
    fn turn_credentials_support_ephemeral_and_configured_modes() -> Result<(), TransportError> {
        let now = Utc::now();
        let ephemeral = TurnServerConfig::new(
            Endpoint::new("turns:turn.example.invalid:5349"),
            Some("joiner".to_owned()),
            Some("ephemeral-secret".to_owned()),
            Some((now + Duration::minutes(5)).to_rfc3339()),
        );
        let configured = TurnServerConfig::new(
            Endpoint::new("turns:turn.example.invalid:5349"),
            Some("configured-user".to_owned()),
            Some("configured-secret".to_owned()),
            None,
        );
        let unauthenticated = TurnServerConfig::new(
            Endpoint::new("turn:open-turn.example.invalid:3478"),
            None,
            None,
            None,
        );

        assert!(matches!(
            ephemeral.credential_mode_at(now)?,
            TurnCredentialMode::Ephemeral { .. }
        ));
        assert_eq!(
            configured.credential_mode_at(now)?,
            TurnCredentialMode::Configured
        );
        assert_eq!(
            unauthenticated.credential_mode_at(now)?,
            TurnCredentialMode::Unauthenticated
        );
        assert!(!format!("{configured:?}").contains("configured-secret"));
        Ok(())
    }

    #[test]
    fn turn_rest_credential_issuer_generates_ephemeral_TURN service_credentials(
    ) -> Result<(), TransportError> {
        let now = DateTime::parse_from_rfc3339("2026-05-29T22:00:00Z")
            .map_err(|err| TransportError::InvalidIcePolicy(err.to_string()))?
            .with_timezone(&Utc);
        let issuer = TurnCredentialIssuer::new(TurnCredentialIssuerConfig::new(
            Endpoint::new("turns:turn.example.invalid:5349"),
            "turn.example.invalid",
            b"0123456789abcdef0123456789abcdef".to_vec(),
            600,
            Some("invite".to_owned()),
        )?)?;

        let issued = issuer.issue_for_subject("opaque-device-1", now)?;
        let repeated = issuer.issue_for_subject("opaque-device-1", now)?;

        assert_eq!(issued, repeated);
        assert_eq!(
            issued.endpoint,
            Endpoint::new("turns:turn.example.invalid:5349")
        );
        assert_eq!(
            issued.username.as_deref(),
            Some("1780092600:invite:opaque-device-1")
        );
        assert_eq!(
            issued.credential_expires_at.as_deref(),
            Some("2026-05-29T22:10:00+00:00")
        );
        assert!(issued
            .credential
            .as_ref()
            .is_some_and(|value| value.len() >= 20));
        assert!(matches!(
            issued.credential_mode_at(now)?,
            TurnCredentialMode::Ephemeral { .. }
        ));
        assert!(!format!("{issued:?}").contains("0123456789abcdef"));
        let issued_credential = issued.credential.as_deref().ok_or_else(|| {
            TransportError::InvalidIcePolicy("issued TURN credential missing".to_owned())
        })?;
        assert!(!format!("{issued:?}").contains(issued_credential));
        Ok(())
    }

    #[test]
    fn turn_credential_issuer_rejects_bad_config_and_subject() -> Result<(), TransportError> {
        assert!(TurnCredentialIssuerConfig::new(
            Endpoint::new("https://turn.example.invalid"),
            "turn.example.invalid",
            b"0123456789abcdef0123456789abcdef".to_vec(),
            600,
            None,
        )
        .is_err());
        assert!(TurnCredentialIssuerConfig::new(
            Endpoint::new("turns:turn.example.invalid:5349"),
            "turn.example.invalid",
            b"short".to_vec(),
            600,
            None,
        )
        .is_err());
        let issuer = TurnCredentialIssuer::new(TurnCredentialIssuerConfig::new(
            Endpoint::new("turns:turn.example.invalid:5349"),
            "turn.example.invalid",
            b"0123456789abcdef0123456789abcdef".to_vec(),
            600,
            None,
        )?)?;
        assert!(issuer.issue_for_subject("not allowed", Utc::now()).is_err());
        Ok(())
    }

    #[test]
    fn turn_stun_credential_decision_covers_adr_004() {
        let decision = turn_stun_credential_decision();

        assert!(decision.covers_adr_004());
        assert!(decision.default_stun_endpoint.starts_with("stun:"));
        assert!(decision.default_turn_endpoint.starts_with("turns:"));
        assert!(decision
            .ephemeral_credential_service
            .contains("base64(HMAC-SHA1"));
    }
}
