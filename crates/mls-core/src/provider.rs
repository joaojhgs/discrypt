//! OpenMLS provider/storage integration and group-operation boundary.
//!
//! The production group path in this crate uses OpenMLS' `MlsGroup` API with
//! the RustCrypto provider and OpenMLS storage.  Higher layers keep raw exporter
//! bytes inside Rust service boundaries and exchange only commits, welcomes,
//! group summaries, and encrypted application data.

use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::OpenMlsRustCrypto;
use std::collections::BTreeMap;
use thiserror::Error;

/// Provider marker describing the current cryptographic backend.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderInfo {
    /// Provider name.
    pub name: &'static str,
    /// Whether this provider is production-audited for release.
    pub production_audited: bool,
}

/// OpenMLS provider metadata.
#[must_use]
pub fn provider_info() -> ProviderInfo {
    ProviderInfo {
        name: "openmls-rustcrypto-memory-storage",
        production_audited: false,
    }
}

/// Errors returned by the OpenMLS group engine wrapper.
#[derive(Debug, Error)]
pub enum OpenMlsEngineError {
    /// The requested group id is not managed by this engine.
    #[error("OpenMLS group {0} not found")]
    GroupNotFound(String),
    /// The requested member label is not present in the group.
    #[error("OpenMLS member {member} not found in group {group_id}")]
    MemberNotFound { group_id: String, member: String },
    /// The requested member already exists in the group.
    #[error("OpenMLS member {member} already exists in group {group_id}")]
    MemberAlreadyExists { group_id: String, member: String },
    /// The OpenMLS provider did not persist the current group state.
    #[error("OpenMLS group {0} was not persisted in provider storage")]
    PersistenceMissing(String),
    /// The persisted group state does not match the in-memory state.
    #[error("OpenMLS persisted group {0} does not match in-memory state")]
    PersistenceMismatch(String),
    /// The OpenMLS API returned an error.
    #[error("OpenMLS operation failed: {0}")]
    OpenMls(String),
}

/// Opaque output from an OpenMLS commit operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenMlsCommitOutput {
    /// Serialized commit message for existing members.
    pub commit: Vec<u8>,
    /// Serialized Welcome message for added members, when applicable.
    pub welcome: Option<Vec<u8>>,
    /// Serialized GroupInfo, when OpenMLS produced one.
    pub group_info: Option<Vec<u8>>,
    /// Group state after the pending commit has been merged locally.
    pub state: OpenMlsGroupState,
}

/// Serializable summary of OpenMLS group state exposed to higher Rust services.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenMlsGroupState {
    /// Stable application group id.
    pub group_id: String,
    /// Current OpenMLS epoch.
    pub epoch: u64,
    /// Active group member labels keyed by OpenMLS leaf index.
    pub members: BTreeMap<u32, String>,
    /// TLS-serialized confirmation tag for convergence checks.
    pub confirmation_tag: Vec<u8>,
}

struct OpenMlsCredential {
    credential_with_key: CredentialWithKey,
    signer: SignatureKeyPair,
}

struct ManagedOpenMlsGroup {
    group: MlsGroup,
    signer: SignatureKeyPair,
}

/// OpenMLS-backed group engine for create/add/remove/rekey/exporter operations.
pub struct OpenMlsGroupEngine {
    provider: OpenMlsRustCrypto,
    ciphersuite: Ciphersuite,
    groups: BTreeMap<String, ManagedOpenMlsGroup>,
}

impl Default for OpenMlsGroupEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenMlsGroupEngine {
    /// Create a new engine with the RustCrypto provider and OpenMLS storage.
    #[must_use]
    pub fn new() -> Self {
        Self {
            provider: OpenMlsRustCrypto::default(),
            ciphersuite: Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            groups: BTreeMap::new(),
        }
    }

    /// Create a new OpenMLS group with `creator` as the initial member.
    pub fn create_group(
        &mut self,
        group_id: impl Into<String>,
        creator: impl Into<String>,
    ) -> Result<OpenMlsGroupState, OpenMlsEngineError> {
        let group_id = group_id.into();
        let creator = creator.into();
        let OpenMlsCredential {
            credential_with_key,
            signer,
        } = self.credential(&creator)?;
        let openmls_group_id = GroupId::from_slice(group_id.as_bytes());
        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &signer,
            &MlsGroupCreateConfig::default(),
            openmls_group_id,
            credential_with_key,
        )
        .map_err(openmls_error)?;
        self.groups
            .insert(group_id.clone(), ManagedOpenMlsGroup { group, signer });
        self.ensure_persisted(&group_id)?;
        self.group_state(&group_id)
    }

    /// Add a member/device as a new OpenMLS leaf and merge the local pending commit.
    pub fn add_member(
        &mut self,
        group_id: &str,
        member: impl Into<String>,
    ) -> Result<OpenMlsCommitOutput, OpenMlsEngineError> {
        let member = member.into();
        if self.member_leaf(group_id, &member)?.is_some() {
            return Err(OpenMlsEngineError::MemberAlreadyExists {
                group_id: group_id.to_owned(),
                member,
            });
        }
        let OpenMlsCredential {
            credential_with_key,
            signer,
        } = self.credential(&member)?;
        let key_package = KeyPackage::builder()
            .build(
                self.ciphersuite,
                &self.provider,
                &signer,
                credential_with_key,
            )
            .map_err(openmls_error)?;
        let provider = &self.provider;
        let managed = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsEngineError::GroupNotFound(group_id.to_owned()))?;
        let (commit, welcome, group_info) = managed
            .group
            .add_members(
                provider,
                &managed.signer,
                core::slice::from_ref(key_package.key_package()),
            )
            .map_err(openmls_error)?;
        managed
            .group
            .merge_pending_commit(provider)
            .map_err(openmls_error)?;
        self.ensure_persisted(group_id)?;
        Ok(OpenMlsCommitOutput {
            commit: serialize_tls(&commit)?,
            welcome: Some(serialize_tls(&welcome)?),
            group_info: group_info.map(|info| serialize_tls(&info)).transpose()?,
            state: self.group_state(group_id)?,
        })
    }

    /// Add another device leaf for the same account label.
    pub fn add_device(
        &mut self,
        group_id: &str,
        account: impl AsRef<str>,
        device_label: impl AsRef<str>,
    ) -> Result<OpenMlsCommitOutput, OpenMlsEngineError> {
        self.add_member(
            group_id,
            format!("{}:{}", account.as_ref(), device_label.as_ref()),
        )
    }

    /// Remove a member/device leaf by its application label and merge the local pending commit.
    pub fn remove_member(
        &mut self,
        group_id: &str,
        member: impl AsRef<str>,
    ) -> Result<OpenMlsCommitOutput, OpenMlsEngineError> {
        let member = member.as_ref();
        let leaf = self.member_leaf(group_id, member)?.ok_or_else(|| {
            OpenMlsEngineError::MemberNotFound {
                group_id: group_id.to_owned(),
                member: member.to_owned(),
            }
        })?;
        let provider = &self.provider;
        let managed = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsEngineError::GroupNotFound(group_id.to_owned()))?;
        let (commit, welcome, group_info) = managed
            .group
            .remove_members(provider, &managed.signer, &[leaf])
            .map_err(openmls_error)?;
        managed
            .group
            .merge_pending_commit(provider)
            .map_err(openmls_error)?;
        self.ensure_persisted(group_id)?;
        Ok(OpenMlsCommitOutput {
            commit: serialize_tls(&commit)?,
            welcome: welcome.map(|message| serialize_tls(&message)).transpose()?,
            group_info: group_info.map(|info| serialize_tls(&info)).transpose()?,
            state: self.group_state(group_id)?,
        })
    }

    /// Remove a device leaf identified by `account:device_label`.
    pub fn remove_device(
        &mut self,
        group_id: &str,
        account: impl AsRef<str>,
        device_label: impl AsRef<str>,
    ) -> Result<OpenMlsCommitOutput, OpenMlsEngineError> {
        self.remove_member(
            group_id,
            format!("{}:{}", account.as_ref(), device_label.as_ref()),
        )
    }

    /// Export secret bytes for Rust-owned text/media/content-key services only.
    pub fn export_secret(
        &self,
        group_id: &str,
        label: &str,
        context: &[u8],
        key_length: usize,
    ) -> Result<Vec<u8>, OpenMlsEngineError> {
        let managed = self.group(group_id)?;
        managed
            .group
            .export_secret(self.provider.crypto(), label, context, key_length)
            .map_err(openmls_error)
    }

    /// Return the current OpenMLS group state summary.
    pub fn group_state(&self, group_id: &str) -> Result<OpenMlsGroupState, OpenMlsEngineError> {
        let managed = self.group(group_id)?;
        Ok(OpenMlsGroupState {
            group_id: group_id.to_owned(),
            epoch: managed.group.epoch().as_u64(),
            members: managed
                .group
                .members()
                .map(|member| {
                    (
                        member.index.u32(),
                        credential_label(&member.credential)
                            .unwrap_or_else(|| format!("member-leaf-{}", member.index.u32())),
                    )
                })
                .collect(),
            confirmation_tag: serialize_tls(managed.group.confirmation_tag())?,
        })
    }

    /// Verify OpenMLS storage can load the same group state.
    pub fn ensure_persisted(&self, group_id: &str) -> Result<(), OpenMlsEngineError> {
        let managed = self.group(group_id)?;
        let loaded = MlsGroup::load(self.provider.storage(), managed.group.group_id())
            .map_err(openmls_error)?
            .ok_or_else(|| OpenMlsEngineError::PersistenceMissing(group_id.to_owned()))?;
        if loaded.epoch() != managed.group.epoch()
            || loaded.confirmation_tag() != managed.group.confirmation_tag()
            || loaded.members().count() != managed.group.members().count()
        {
            return Err(OpenMlsEngineError::PersistenceMismatch(group_id.to_owned()));
        }
        Ok(())
    }

    fn credential(&self, label: &str) -> Result<OpenMlsCredential, OpenMlsEngineError> {
        let credential = BasicCredential::new(label.as_bytes().to_vec());
        let signer =
            SignatureKeyPair::new(self.ciphersuite.signature_algorithm()).map_err(openmls_error)?;
        signer
            .store(self.provider.storage())
            .map_err(openmls_error)?;
        Ok(OpenMlsCredential {
            credential_with_key: CredentialWithKey {
                credential: credential.into(),
                signature_key: signer.public().into(),
            },
            signer,
        })
    }

    fn group(&self, group_id: &str) -> Result<&ManagedOpenMlsGroup, OpenMlsEngineError> {
        self.groups
            .get(group_id)
            .ok_or_else(|| OpenMlsEngineError::GroupNotFound(group_id.to_owned()))
    }

    fn member_leaf(
        &self,
        group_id: &str,
        member: &str,
    ) -> Result<Option<LeafNodeIndex>, OpenMlsEngineError> {
        Ok(self.group(group_id)?.group.members().find_map(|candidate| {
            (credential_label(&candidate.credential).as_deref() == Some(member))
                .then_some(candidate.index)
        }))
    }
}

fn serialize_tls<T: openmls::prelude::tls_codec::Serialize>(
    value: &T,
) -> Result<Vec<u8>, OpenMlsEngineError> {
    value.tls_serialize_detached().map_err(openmls_error)
}

fn credential_label(credential: &Credential) -> Option<String> {
    BasicCredential::try_from(credential.clone())
        .ok()
        .and_then(|basic| String::from_utf8(basic.identity().to_vec()).ok())
}

fn openmls_error(error: impl std::fmt::Debug) -> OpenMlsEngineError {
    OpenMlsEngineError::OpenMls(format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openmls_create_add_remove_member_rekeys_and_persists() -> Result<(), OpenMlsEngineError> {
        let mut engine = OpenMlsGroupEngine::new();
        let created = engine.create_group("room", "alice:laptop")?;
        assert_eq!(created.epoch, 0);
        assert_eq!(
            created.members.values().collect::<Vec<_>>(),
            vec!["alice:laptop"]
        );
        engine.ensure_persisted("room")?;

        let before_add = engine.export_secret("room", "discrypt-content", b"text", 32)?;
        let add = engine.add_member("room", "bob:laptop")?;
        assert_eq!(add.state.epoch, 1);
        assert_eq!(add.state.members.len(), 2);
        assert!(add.welcome.is_some());
        assert!(!add.commit.is_empty());
        assert_ne!(
            before_add,
            engine.export_secret("room", "discrypt-content", b"text", 32)?
        );
        engine.ensure_persisted("room")?;

        let after_add = engine.export_secret("room", "discrypt-media", b"sframe", 32)?;
        let remove = engine.remove_member("room", "bob:laptop")?;
        assert_eq!(remove.state.epoch, 2);
        assert_eq!(
            remove.state.members.values().collect::<Vec<_>>(),
            vec!["alice:laptop"]
        );
        assert_ne!(
            after_add,
            engine.export_secret("room", "discrypt-media", b"sframe", 32)?
        );
        engine.ensure_persisted("room")?;
        Ok(())
    }

    #[test]
    fn openmls_add_remove_device_uses_distinct_leaves() -> Result<(), OpenMlsEngineError> {
        let mut engine = OpenMlsGroupEngine::new();
        engine.create_group("room-devices", "alice:laptop")?;
        let add = engine.add_device("room-devices", "alice", "phone")?;
        assert_eq!(add.state.members.len(), 2);
        assert!(add
            .state
            .members
            .values()
            .any(|member| member == "alice:laptop"));
        assert!(add
            .state
            .members
            .values()
            .any(|member| member == "alice:phone"));
        assert_ne!(
            add.state
                .members
                .iter()
                .find_map(|(leaf, member)| (member == "alice:laptop").then_some(*leaf)),
            add.state
                .members
                .iter()
                .find_map(|(leaf, member)| (member == "alice:phone").then_some(*leaf))
        );

        let remove = engine.remove_device("room-devices", "alice", "phone")?;
        assert_eq!(remove.state.epoch, 2);
        assert_eq!(
            remove.state.members.values().collect::<Vec<_>>(),
            vec!["alice:laptop"]
        );
        assert!(matches!(
            engine.remove_device("room-devices", "alice", "phone"),
            Err(OpenMlsEngineError::MemberNotFound { .. })
        ));
        Ok(())
    }

    #[test]
    fn openmls_exporter_is_stable_for_same_epoch_and_domain_separated(
    ) -> Result<(), OpenMlsEngineError> {
        let mut engine = OpenMlsGroupEngine::new();
        engine.create_group("room-export", "alice:laptop")?;
        let content = engine.export_secret("room-export", "discrypt-content", b"text", 32)?;
        assert_eq!(
            content,
            engine.export_secret("room-export", "discrypt-content", b"text", 32)?
        );
        assert_ne!(
            content,
            engine.export_secret("room-export", "discrypt-content", b"voice", 32)?
        );
        assert_ne!(
            content,
            engine.export_secret("room-export", "discrypt-media", b"text", 32)?
        );
        Ok(())
    }
}
