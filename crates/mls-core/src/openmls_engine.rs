//! OpenMLS-backed group engine and persistent provider integration.
//!
//! This module is the Phase-D production-facing MLS boundary. It uses the
//! upstream OpenMLS group APIs with the RustCrypto provider and the OpenMLS
//! SQLite storage provider, so group state, epochs, queued proposals, pending
//! commits, epoch secrets, and confirmation tags are written through OpenMLS'
//! `StorageProvider` instead of the legacy deterministic boundary.

use openmls::prelude::{
    tls_codec::{Deserialize as TlsDeserializeTrait, Serialize as TlsSerializeTrait},
    BasicCredential, Ciphersuite, Credential, CredentialWithKey, Extensions,
    GroupId as OpenMlsGroupId, KeyPackage, KeyPackageIn, LeafNodeIndex, MlsGroup,
    MlsGroupCreateConfig, MlsGroupJoinConfig, MlsMessageBodyOut, MlsMessageIn, MlsMessageOut,
    ProcessedMessageContent, ProtocolMessage, StagedWelcome, Welcome,
};
use openmls::versions::ProtocolVersion;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::RustCrypto;
use openmls_sqlite_storage::{Codec, Connection, SqliteStorageProvider};
use openmls_traits::{types::SignatureScheme, OpenMlsProvider};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// OpenMLS storage codec used for the SQLite provider.
#[derive(Clone, Debug, Default)]
pub struct JsonOpenMlsCodec;

impl Codec for JsonOpenMlsCodec {
    type Error = serde_json::Error;

    fn to_vec<T: Serialize>(value: &T) -> Result<Vec<u8>, Self::Error> {
        serde_json::to_vec(value)
    }

    fn from_slice<T: DeserializeOwned>(slice: &[u8]) -> Result<T, Self::Error> {
        serde_json::from_slice(slice)
    }
}

type OpenMlsSqliteStorage = SqliteStorageProvider<JsonOpenMlsCodec, Connection>;

/// RustCrypto + SQLite OpenMLS provider used by Discrypt group services.
pub struct DiscryptOpenMlsProvider {
    storage: OpenMlsSqliteStorage,
    crypto: RustCrypto,
    path: PathBuf,
}

impl std::fmt::Debug for DiscryptOpenMlsProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscryptOpenMlsProvider")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

impl DiscryptOpenMlsProvider {
    /// Open or create an OpenMLS SQLite storage provider and run migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenMlsGroupError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(&path)
            .map_err(|error| OpenMlsGroupError::OpenMls(format!("sqlite open failed: {error}")))?;
        let mut storage = SqliteStorageProvider::<JsonOpenMlsCodec, _>::new(connection);
        storage.run_migrations().map_err(|error| {
            OpenMlsGroupError::OpenMls(format!("sqlite migration failed: {error}"))
        })?;
        Ok(Self {
            storage,
            crypto: RustCrypto::default(),
            path,
        })
    }

    /// Return the SQLite path used by the provider.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl OpenMlsProvider for DiscryptOpenMlsProvider {
    type CryptoProvider = RustCrypto;
    type RandProvider = RustCrypto;
    type StorageProvider = OpenMlsSqliteStorage;

    fn storage(&self) -> &Self::StorageProvider {
        &self.storage
    }

    fn crypto(&self) -> &Self::CryptoProvider {
        &self.crypto
    }

    fn rand(&self) -> &Self::RandProvider {
        &self.crypto
    }
}

/// Errors raised by the OpenMLS group engine.
#[derive(Debug, Error)]
pub enum OpenMlsGroupError {
    /// Filesystem or directory setup failed.
    #[error("openmls provider io error: {0}")]
    Io(#[from] std::io::Error),
    /// JSON storage codec failed.
    #[error("openmls storage codec error: {0}")]
    Codec(#[from] serde_json::Error),
    /// Upstream OpenMLS operation failed.
    #[error("openmls operation failed: {0}")]
    OpenMls(String),
    /// Group does not exist in live or persistent OpenMLS storage.
    #[error("openmls group {0} not found")]
    GroupNotFound(String),
    /// The requested member label is not present in the group.
    #[error("openmls member {member} not found in group {group_id}")]
    MemberNotFound { group_id: String, member: String },
    /// The requested member label is already present in the group.
    #[error("openmls member {member} already exists in group {group_id}")]
    MemberAlreadyExists { group_id: String, member: String },
    /// Signature key material for a persisted group was not available.
    #[error("openmls signer key not found for group {group_id}")]
    SignerNotFound { group_id: String },
    /// Commit bytes did not match the staged OpenMLS commit.
    #[error("commit bytes do not match the pending openmls commit for epoch {0}")]
    CommitMismatch(u64),
    /// Commit epoch was not the expected next OpenMLS epoch.
    #[error("commit epoch {attempted} does not follow current epoch {current}")]
    StaleCommitEpoch { current: u64, attempted: u64 },
    /// The Welcome joined a different OpenMLS group than the expected group id.
    #[error("welcome group id {actual} does not match expected group id {expected}")]
    WelcomeGroupIdMismatch { expected: String, actual: String },
    /// The delivered OpenMLS message was not the expected commit message.
    #[error("openmls message was not a staged commit for group {0}")]
    UnexpectedCommitMessage(String),
}

/// Snapshot of durable OpenMLS group state used by higher-level service seams.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenMlsGroupSnapshot {
    /// Stable Discrypt/OpenMLS group id.
    pub group_id: String,
    /// Current accepted MLS epoch.
    pub epoch: u64,
    /// Serialized OpenMLS confirmation tag for the current epoch.
    pub confirmation_tag: Vec<u8>,
    /// Count of pending OpenMLS proposals in provider storage/group state.
    pub pending_proposals: usize,
    /// Whether an OpenMLS commit is staged and awaiting merge.
    pub pending_commit: bool,
}

/// Output from a merged OpenMLS group operation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OpenMlsGroupOperationResult {
    /// Serialized commit message for current members.
    pub commit: Vec<u8>,
    /// Serialized Welcome message for added members/devices, if OpenMLS produced one.
    pub welcome: Option<Vec<u8>>,
    /// Serialized GroupInfo for joiner validation, if OpenMLS produced one.
    pub group_info: Option<Vec<u8>>,
    /// Group state after the local pending commit is merged.
    pub state: OpenMlsGroupSnapshot,
}

/// A generated OpenMLS member key package and signer handle.
pub struct OpenMlsMemberPackage {
    key_package: KeyPackage,
    signer_public_key: Vec<u8>,
}

impl OpenMlsMemberPackage {
    /// Public key bytes used to reload this member's signer from OpenMLS storage.
    #[must_use]
    pub fn signer_public_key(&self) -> &[u8] {
        &self.signer_public_key
    }

    /// TLS-serialized OpenMLS key package bytes for transport over an admission channel.
    ///
    /// The signer handle is intentionally not embedded here; callers must carry
    /// `signer_public_key()` alongside these bytes so the joiner can reload its
    /// own private signer after receiving a Welcome.
    ///
    /// # Errors
    ///
    /// Returns an error when OpenMLS cannot serialize the key package.
    pub fn key_package_bytes(&self) -> Result<Vec<u8>, OpenMlsGroupError> {
        serialize_tls(&self.key_package)
    }

    /// Rehydrate a member package received over an authenticated admission channel.
    ///
    /// # Errors
    ///
    /// Returns an error when the key package bytes are not valid OpenMLS TLS
    /// encoding.
    pub fn from_key_package_bytes(
        signer_public_key: Vec<u8>,
        key_package_bytes: &[u8],
    ) -> Result<Self, OpenMlsGroupError> {
        let mut encoded = key_package_bytes;
        let key_package_in = KeyPackageIn::tls_deserialize(&mut encoded).map_err(openmls_error)?;
        let crypto = RustCrypto::default();
        let key_package = key_package_in
            .validate(&crypto, ProtocolVersion::default())
            .map_err(openmls_error)?;
        Ok(Self {
            key_package,
            signer_public_key,
        })
    }
}

struct OpenMlsTrackedGroup {
    group: MlsGroup,
    signer_public_key: Vec<u8>,
    signer: SignatureKeyPair,
    pending_commit: Option<Vec<u8>>,
}

/// Stateful OpenMLS group engine backed by persistent SQLite storage.
pub struct OpenMlsGroupEngine {
    provider: DiscryptOpenMlsProvider,
    groups: BTreeMap<String, OpenMlsTrackedGroup>,
    ciphersuite: Ciphersuite,
    signature_scheme: SignatureScheme,
}

impl OpenMlsGroupEngine {
    /// Open an engine using an OpenMLS SQLite storage database.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, OpenMlsGroupError> {
        Ok(Self {
            provider: DiscryptOpenMlsProvider::open(path)?,
            groups: BTreeMap::new(),
            ciphersuite: Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519,
            signature_scheme: SignatureScheme::ED25519,
        })
    }

    /// Return the underlying provider path.
    #[must_use]
    pub fn provider_path(&self) -> &Path {
        self.provider.path()
    }

    /// Create a real OpenMLS group and persist it through the configured storage provider.
    pub fn create_group(
        &mut self,
        group_id: impl AsRef<str>,
        creator_identity: impl AsRef<[u8]>,
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let group_id_string = group_id.as_ref().to_owned();
        let (credential, signer) = self.generate_credential(creator_identity.as_ref())?;
        let signer_public_key = signer.to_public_vec();
        let group_config = MlsGroupCreateConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        let group = MlsGroup::new_with_group_id(
            &self.provider,
            &signer,
            &group_config,
            OpenMlsGroupId::from_slice(group_id_string.as_bytes()),
            credential,
        )
        .map_err(openmls_error)?;
        let tracked = OpenMlsTrackedGroup {
            group,
            signer_public_key,
            signer,
            pending_commit: None,
        };
        let snapshot = snapshot(&tracked.group, &tracked.pending_commit)?;
        self.groups.insert(group_id_string, tracked);
        Ok(snapshot)
    }

    /// Generate and persist a member key package using the engine provider.
    pub fn generate_member_package(
        &self,
        identity: impl AsRef<[u8]>,
    ) -> Result<OpenMlsMemberPackage, OpenMlsGroupError> {
        let (credential, signer) = self.generate_credential(identity.as_ref())?;
        let signer_public_key = signer.to_public_vec();
        let key_package = KeyPackage::builder()
            .key_package_extensions(Extensions::empty())
            .build(self.ciphersuite, &self.provider, &signer, credential)
            .map_err(openmls_error)?;
        Ok(OpenMlsMemberPackage {
            key_package: key_package.key_package().clone(),
            signer_public_key,
        })
    }

    /// Stage an OpenMLS add-member commit and persist the pending commit/group state.
    pub fn stage_add_member(
        &mut self,
        group_id: &str,
        member: &OpenMlsMemberPackage,
    ) -> Result<Vec<u8>, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let (commit, _welcome, _group_info) = tracked
            .group
            .add_members(
                &self.provider,
                &tracked.signer,
                std::slice::from_ref(&member.key_package),
            )
            .map_err(openmls_error)?;
        let commit_bytes = commit.tls_serialize_detached().map_err(openmls_error)?;
        tracked.pending_commit = Some(commit_bytes.clone());
        Ok(commit_bytes)
    }

    /// Add a member/device leaf, merge the pending commit locally, and return commit artifacts.
    pub fn add_member(
        &mut self,
        group_id: &str,
        member_identity: impl AsRef<[u8]>,
    ) -> Result<OpenMlsGroupOperationResult, OpenMlsGroupError> {
        let member_label = String::from_utf8_lossy(member_identity.as_ref()).into_owned();
        if self.member_leaf(group_id, &member_label)?.is_some() {
            return Err(OpenMlsGroupError::MemberAlreadyExists {
                group_id: group_id.to_owned(),
                member: member_label,
            });
        }
        let member = self.generate_member_package(member_identity)?;
        self.add_member_package(group_id, &member)
    }

    /// Add a leaf using a key package generated by the joining device's provider.
    pub fn add_member_package(
        &mut self,
        group_id: &str,
        member: &OpenMlsMemberPackage,
    ) -> Result<OpenMlsGroupOperationResult, OpenMlsGroupError> {
        let member_label = credential_label(member.key_package.leaf_node().credential())
            .ok_or_else(|| {
                OpenMlsGroupError::OpenMls(
                    "member key package is missing a decodable BasicCredential label".to_owned(),
                )
            })?;
        if self.member_leaf(group_id, &member_label)?.is_some() {
            return Err(OpenMlsGroupError::MemberAlreadyExists {
                group_id: group_id.to_owned(),
                member: member_label,
            });
        }
        let tracked = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let (commit, welcome, group_info) = tracked
            .group
            .add_members(
                &self.provider,
                &tracked.signer,
                std::slice::from_ref(&member.key_package),
            )
            .map_err(openmls_error)?;
        let commit_bytes = serialize_tls(&commit)?;
        let welcome_bytes = Some(welcome_bytes_from_message(&welcome)?);
        let group_info_bytes = group_info.map(|info| serialize_tls(&info)).transpose()?;
        tracked.pending_commit = Some(commit_bytes.clone());
        tracked
            .group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;
        tracked.pending_commit = None;
        Ok(OpenMlsGroupOperationResult {
            commit: commit_bytes,
            welcome: welcome_bytes,
            group_info: group_info_bytes,
            state: snapshot(&tracked.group, &tracked.pending_commit)?,
        })
    }

    /// Join a group from a cryptographically validated OpenMLS Welcome.
    pub fn join_from_welcome(
        &mut self,
        expected_group_id: &str,
        signer_public_key: &[u8],
        welcome_bytes: &[u8],
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let mut encoded = welcome_bytes;
        let welcome = Welcome::tls_deserialize(&mut encoded).map_err(openmls_error)?;
        let join_config = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .build();
        let group = StagedWelcome::new_from_welcome(&self.provider, &join_config, welcome, None)
            .map_err(openmls_error)?
            .into_group(&self.provider)
            .map_err(openmls_error)?;
        let actual_group_id = String::from_utf8_lossy(group.group_id().as_slice()).into_owned();
        if actual_group_id != expected_group_id {
            return Err(OpenMlsGroupError::WelcomeGroupIdMismatch {
                expected: expected_group_id.to_owned(),
                actual: actual_group_id,
            });
        }
        let signer = SignatureKeyPair::read(
            self.provider.storage(),
            signer_public_key,
            self.signature_scheme,
        )
        .ok_or_else(|| OpenMlsGroupError::SignerNotFound {
            group_id: expected_group_id.to_owned(),
        })?;
        let tracked = OpenMlsTrackedGroup {
            group,
            signer_public_key: signer_public_key.to_vec(),
            signer,
            pending_commit: None,
        };
        let snapshot = snapshot(&tracked.group, &tracked.pending_commit)?;
        self.groups.insert(expected_group_id.to_owned(), tracked);
        Ok(snapshot)
    }

    /// Add another device leaf for the same account label.
    pub fn add_device(
        &mut self,
        group_id: &str,
        account: impl AsRef<str>,
        device_label: impl AsRef<str>,
    ) -> Result<OpenMlsGroupOperationResult, OpenMlsGroupError> {
        self.add_member(
            group_id,
            format!("{}:{}", account.as_ref(), device_label.as_ref()),
        )
    }

    /// Remove a member/device leaf, merge the pending rekey commit locally, and return artifacts.
    pub fn remove_member(
        &mut self,
        group_id: &str,
        member_identity: impl AsRef<str>,
    ) -> Result<OpenMlsGroupOperationResult, OpenMlsGroupError> {
        let member = member_identity.as_ref();
        let leaf = self.member_leaf(group_id, member)?.ok_or_else(|| {
            OpenMlsGroupError::MemberNotFound {
                group_id: group_id.to_owned(),
                member: member.to_owned(),
            }
        })?;
        let tracked = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let (commit, welcome, group_info) = tracked
            .group
            .remove_members(&self.provider, &tracked.signer, &[leaf])
            .map_err(openmls_error)?;
        let commit_bytes = serialize_tls(&commit)?;
        let welcome_bytes = welcome
            .as_ref()
            .map(welcome_bytes_from_message)
            .transpose()?;
        let group_info_bytes = group_info.map(|info| serialize_tls(&info)).transpose()?;
        tracked.pending_commit = Some(commit_bytes.clone());
        tracked
            .group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;
        tracked.pending_commit = None;
        Ok(OpenMlsGroupOperationResult {
            commit: commit_bytes,
            welcome: welcome_bytes,
            group_info: group_info_bytes,
            state: snapshot(&tracked.group, &tracked.pending_commit)?,
        })
    }

    /// Remove a device leaf identified by `account:device_label`.
    pub fn remove_device(
        &mut self,
        group_id: &str,
        account: impl AsRef<str>,
        device_label: impl AsRef<str>,
    ) -> Result<OpenMlsGroupOperationResult, OpenMlsGroupError> {
        self.remove_member(
            group_id,
            format!("{}:{}", account.as_ref(), device_label.as_ref()),
        )
    }

    /// Merge the currently pending OpenMLS commit after matching the delivered commit bytes.
    pub fn merge_pending_commit(
        &mut self,
        group_id: &str,
        expected_epoch: u64,
        commit_bytes: &[u8],
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let current = tracked.group.epoch().as_u64();
        if expected_epoch != current.saturating_add(1) {
            return Err(OpenMlsGroupError::StaleCommitEpoch {
                current,
                attempted: expected_epoch,
            });
        }
        if tracked.pending_commit.as_deref() != Some(commit_bytes) {
            return Err(OpenMlsGroupError::CommitMismatch(expected_epoch));
        }
        tracked
            .group
            .merge_pending_commit(&self.provider)
            .map_err(openmls_error)?;
        tracked.pending_commit = None;
        snapshot(&tracked.group, &tracked.pending_commit)
    }

    /// Apply a delivered OpenMLS commit from another member and persist the new epoch.
    pub fn apply_external_commit(
        &mut self,
        group_id: &str,
        expected_epoch: u64,
        commit_bytes: &[u8],
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        self.apply_external_commit_with_remove_target(group_id, expected_epoch, commit_bytes, None)
    }

    /// Apply a delivered OpenMLS remove-member commit and prove that it removes the expected leaf.
    pub fn apply_external_remove_commit(
        &mut self,
        group_id: &str,
        expected_epoch: u64,
        commit_bytes: &[u8],
        removed_member_identity: impl AsRef<str>,
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let removed_member = removed_member_identity.as_ref();
        let target_leaf = self.member_leaf(group_id, removed_member)?.ok_or_else(|| {
            OpenMlsGroupError::MemberNotFound {
                group_id: group_id.to_owned(),
                member: removed_member.to_owned(),
            }
        })?;
        self.apply_external_commit_with_remove_target(
            group_id,
            expected_epoch,
            commit_bytes,
            Some((removed_member, target_leaf)),
        )
    }

    fn apply_external_commit_with_remove_target(
        &mut self,
        group_id: &str,
        expected_epoch: u64,
        commit_bytes: &[u8],
        required_remove_target: Option<(&str, LeafNodeIndex)>,
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get_mut(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let current = tracked.group.epoch().as_u64();
        if expected_epoch != current.saturating_add(1) {
            return Err(OpenMlsGroupError::StaleCommitEpoch {
                current,
                attempted: expected_epoch,
            });
        }
        let mut encoded = commit_bytes;
        let message = MlsMessageIn::tls_deserialize(&mut encoded)
            .map_err(openmls_error)?
            .try_into_protocol_message()
            .map_err(openmls_error)?;
        let processed = tracked
            .group
            .process_message(&self.provider, message)
            .map_err(openmls_error)?;
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(commit) => {
                if let Some((removed_member, target_leaf)) = required_remove_target {
                    let removes_expected_leaf = commit
                        .remove_proposals()
                        .any(|proposal| proposal.remove_proposal().removed() == target_leaf);
                    if !removes_expected_leaf {
                        return Err(OpenMlsGroupError::UnexpectedCommitMessage(format!(
                            "{group_id}:remove_target_mismatch:{removed_member}"
                        )));
                    }
                }
                tracked
                    .group
                    .merge_staged_commit(&self.provider, *commit)
                    .map_err(openmls_error)?;
                snapshot(&tracked.group, &tracked.pending_commit)
            }
            _ => Err(OpenMlsGroupError::UnexpectedCommitMessage(
                group_id.to_owned(),
            )),
        }
    }

    /// Hash the confirmation tag carried by a serialized public commit before merging it.
    ///
    /// Higher layers use this as a fail-closed metadata check so a frame with a
    /// forged epoch/tag cannot mutate durable OpenMLS state and then be reported
    /// as rejected only after the merge.
    pub fn public_commit_confirmation_tag_sha256(
        commit_bytes: &[u8],
    ) -> Result<Option<String>, OpenMlsGroupError> {
        let mut encoded = commit_bytes;
        let message = MlsMessageIn::tls_deserialize(&mut encoded)
            .map_err(openmls_error)?
            .try_into_protocol_message()
            .map_err(openmls_error)?;
        let ProtocolMessage::PublicMessage(public_message) = message else {
            return Ok(None);
        };
        let confirmation_tag = public_message
            .confirmation_tag()
            .ok_or_else(|| OpenMlsGroupError::UnexpectedCommitMessage("missing_tag".to_owned()))?;
        let confirmation_bytes = confirmation_tag
            .tls_serialize_detached()
            .map_err(openmls_error)?;
        let mut hasher = Sha256::new();
        hasher.update(&confirmation_bytes);
        Ok(Some(hex::encode(hasher.finalize())))
    }

    /// Export secret material from the current OpenMLS epoch.
    pub fn export_secret(
        &self,
        group_id: &str,
        label: &str,
        context: &[u8],
        key_length: usize,
    ) -> Result<Vec<u8>, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        tracked
            .group
            .export_secret(self.provider.crypto(), label, context, key_length)
            .map_err(openmls_error)
    }

    /// Load a persisted group from OpenMLS storage and rehydrate its signer.
    pub fn load_group(
        &mut self,
        group_id: &str,
        signer_public_key: &[u8],
    ) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let openmls_group_id = OpenMlsGroupId::from_slice(group_id.as_bytes());
        let group = MlsGroup::load(self.provider.storage(), &openmls_group_id)
            .map_err(openmls_error)?
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        let signer = SignatureKeyPair::read(
            self.provider.storage(),
            signer_public_key,
            self.signature_scheme,
        )
        .ok_or_else(|| OpenMlsGroupError::SignerNotFound {
            group_id: group_id.to_owned(),
        })?;
        let pending_commit = group.pending_commit().map(|_| Vec::new());
        let tracked = OpenMlsTrackedGroup {
            group,
            signer_public_key: signer_public_key.to_vec(),
            signer,
            pending_commit,
        };
        let snapshot = snapshot(&tracked.group, &tracked.pending_commit)?;
        self.groups.insert(group_id.to_owned(), tracked);
        Ok(snapshot)
    }

    /// Snapshot a live group.
    pub fn snapshot(&self, group_id: &str) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        snapshot(&tracked.group, &tracked.pending_commit)
    }

    /// Return backend-observed member credential labels for a live group.
    ///
    /// Higher layers use these labels as evidence that device membership came
    /// from OpenMLS group state instead of frontend-only device labels.
    pub fn member_labels(&self, group_id: &str) -> Result<Vec<String>, OpenMlsGroupError> {
        let tracked = self
            .groups
            .get(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?;
        Ok(tracked
            .group
            .members()
            .filter_map(|member| credential_label(&member.credential))
            .collect())
    }

    /// Return the creator/signer public key for a live group so callers can persist handles.
    pub fn signer_public_key(&self, group_id: &str) -> Result<Vec<u8>, OpenMlsGroupError> {
        self.groups
            .get(group_id)
            .map(|tracked| tracked.signer_public_key.clone())
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))
    }

    fn generate_credential(
        &self,
        identity: &[u8],
    ) -> Result<(CredentialWithKey, SignatureKeyPair), OpenMlsGroupError> {
        let credential = BasicCredential::new(identity.to_vec());
        let signer = SignatureKeyPair::new(self.signature_scheme).map_err(openmls_error)?;
        signer
            .store(self.provider.storage())
            .map_err(openmls_error)?;
        Ok((
            CredentialWithKey {
                credential: credential.into(),
                signature_key: signer.to_public_vec().into(),
            },
            signer,
        ))
    }

    fn member_leaf(
        &self,
        group_id: &str,
        member: &str,
    ) -> Result<Option<LeafNodeIndex>, OpenMlsGroupError> {
        Ok(self
            .groups
            .get(group_id)
            .ok_or_else(|| OpenMlsGroupError::GroupNotFound(group_id.to_owned()))?
            .group
            .members()
            .find_map(|candidate| {
                (credential_label(&candidate.credential).as_deref() == Some(member))
                    .then_some(candidate.index)
            }))
    }
}

fn serialize_tls<T: TlsSerializeTrait>(value: &T) -> Result<Vec<u8>, OpenMlsGroupError> {
    value.tls_serialize_detached().map_err(openmls_error)
}

fn welcome_bytes_from_message(message: &MlsMessageOut) -> Result<Vec<u8>, OpenMlsGroupError> {
    match message.body() {
        MlsMessageBodyOut::Welcome(welcome) => serialize_tls(welcome),
        _ => Err(OpenMlsGroupError::OpenMls(
            "OpenMLS operation did not return a Welcome message".to_owned(),
        )),
    }
}

fn credential_label(credential: &Credential) -> Option<String> {
    BasicCredential::try_from(credential.clone())
        .ok()
        .and_then(|basic| String::from_utf8(basic.identity().to_vec()).ok())
}

fn snapshot(
    group: &MlsGroup,
    pending_commit: &Option<Vec<u8>>,
) -> Result<OpenMlsGroupSnapshot, OpenMlsGroupError> {
    Ok(OpenMlsGroupSnapshot {
        group_id: String::from_utf8_lossy(group.group_id().as_slice()).into_owned(),
        epoch: group.epoch().as_u64(),
        confirmation_tag: group
            .confirmation_tag()
            .tls_serialize_detached()
            .map_err(openmls_error)?,
        pending_proposals: group.pending_proposals().count(),
        pending_commit: pending_commit.is_some() || group.pending_commit().is_some(),
    })
}

fn openmls_error(error: impl std::fmt::Debug) -> OpenMlsGroupError {
    OpenMlsGroupError::OpenMls(format!("{error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DeviceSet, Identity};
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "discrypt-openmls-{name}-{}-{}.sqlite",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
        ))
    }

    #[test]
    fn openmls_group_create_add_merge_export_and_reload() -> Result<(), OpenMlsGroupError> {
        let path = temp_path("create-add-merge");
        let mut engine = OpenMlsGroupEngine::open(&path)?;
        let created = engine.create_group("room-openmls", b"alice")?;
        assert_eq!(created.epoch, 0);
        assert!(!created.confirmation_tag.is_empty());
        let signer_public_key = engine.signer_public_key("room-openmls")?;
        let before = engine.export_secret("room-openmls", "discrypt/text", b"room", 32)?;

        let bob = engine.generate_member_package(b"bob")?;
        let commit = engine.stage_add_member("room-openmls", &bob)?;
        let staged = engine.snapshot("room-openmls")?;
        assert_eq!(staged.epoch, 0);
        assert!(staged.pending_commit);

        let merged = engine.merge_pending_commit("room-openmls", 1, &commit)?;
        assert_eq!(merged.epoch, 1);
        assert!(!merged.pending_commit);
        assert_ne!(created.confirmation_tag, merged.confirmation_tag);
        let after = engine.export_secret("room-openmls", "discrypt/text", b"room", 32)?;
        assert_ne!(before, after);
        drop(engine);

        let mut reloaded = OpenMlsGroupEngine::open(&path)?;
        let reloaded_snapshot = reloaded.load_group("room-openmls", &signer_public_key)?;
        assert_eq!(reloaded_snapshot.epoch, merged.epoch);
        assert_eq!(reloaded_snapshot.confirmation_tag, merged.confirmation_tag);
        let reloaded_secret =
            reloaded.export_secret("room-openmls", "discrypt/text", b"room", 32)?;
        assert_eq!(reloaded_secret, after);

        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn openmls_add_remove_member_and_device_rekey() -> Result<(), OpenMlsGroupError> {
        let path = temp_path("add-remove");
        let mut engine = OpenMlsGroupEngine::open(&path)?;
        let created = engine.create_group("room-members", b"alice:laptop")?;
        let before = engine.export_secret("room-members", "discrypt/v1/text", b"room", 32)?;

        let added = engine.add_member("room-members", b"bob:laptop")?;
        assert_eq!(added.state.epoch, created.epoch + 1);
        assert!(added.welcome.is_some());
        assert!(!added.commit.is_empty());
        assert_ne!(
            before,
            engine.export_secret("room-members", "discrypt/v1/text", b"room", 32)?
        );

        let device = engine.add_device("room-members", "alice", "phone")?;
        assert_eq!(device.state.epoch, added.state.epoch + 1);
        assert!(device.welcome.is_some());

        let removed_device = engine.remove_device("room-members", "alice", "phone")?;
        assert_eq!(removed_device.state.epoch, device.state.epoch + 1);
        assert!(removed_device.welcome.is_none());

        let removed_member = engine.remove_member("room-members", "bob:laptop")?;
        assert_eq!(removed_member.state.epoch, removed_device.state.epoch + 1);
        assert!(matches!(
            engine.remove_member("room-members", "bob:laptop"),
            Err(OpenMlsGroupError::MemberNotFound { .. })
        ));

        let _ = std::fs::remove_file(path);
        Ok(())
    }

    #[test]
    fn two_device_profile_pairing_join_reload_and_remove_rekey(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let alice_path = temp_path("per80-alice-laptop");
        let phone_path = temp_path("per80-alice-phone");
        let mut alice_laptop = OpenMlsGroupEngine::open(&alice_path)?;
        let mut alice_phone = OpenMlsGroupEngine::open(&phone_path)?;

        let identity = Identity::generate("alice");
        let mut device_set = DeviceSet::new();
        let laptop_leaf = device_set.add_authorized_device(
            &identity,
            SigningKey::generate(&mut OsRng).verifying_key(),
            "laptop",
            0,
        );
        let created = alice_laptop.create_group("room-two-device", b"alice:laptop")?;
        assert_eq!(created.epoch, 0);

        let pairing_payload =
            device_set.create_pairing_payload(&identity, laptop_leaf.device_id, "phone", 0, 3)?;
        let phone_leaf = device_set.add_device_from_pairing_payload(
            &identity,
            &pairing_payload,
            SigningKey::generate(&mut OsRng).verifying_key(),
            1,
        )?;
        assert_eq!(phone_leaf.leaf_index, 1);
        assert_eq!(laptop_leaf.identity_key, phone_leaf.identity_key);
        assert_ne!(laptop_leaf.device_key, phone_leaf.device_key);

        let phone_package = alice_phone.generate_member_package(b"alice:phone")?;
        let added_phone = alice_laptop.add_member_package("room-two-device", &phone_package)?;
        let phone_welcome = added_phone.welcome.as_ref().ok_or_else(|| {
            OpenMlsGroupError::OpenMls("OpenMLS add did not produce phone welcome".to_owned())
        })?;
        let joined_phone = alice_phone.join_from_welcome(
            "room-two-device",
            phone_package.signer_public_key(),
            phone_welcome,
        )?;
        assert_eq!(joined_phone.epoch, added_phone.state.epoch);
        assert_eq!(
            joined_phone.confirmation_tag,
            added_phone.state.confirmation_tag
        );
        assert_eq!(
            alice_laptop.member_labels("room-two-device")?,
            vec!["alice:laptop".to_owned(), "alice:phone".to_owned()]
        );
        assert_eq!(
            alice_phone.export_secret("room-two-device", "discrypt/v1/text", b"room", 32)?,
            alice_laptop.export_secret("room-two-device", "discrypt/v1/text", b"room", 32)?
        );

        let serialized_devices = serde_json::to_vec(&device_set)?;
        drop(alice_phone);
        let mut reloaded_phone = OpenMlsGroupEngine::open(&phone_path)?;
        let reloaded_joined =
            reloaded_phone.load_group("room-two-device", phone_package.signer_public_key())?;
        assert_eq!(reloaded_joined.epoch, added_phone.state.epoch);
        assert_eq!(
            reloaded_joined.confirmation_tag,
            added_phone.state.confirmation_tag
        );
        let reloaded_devices: DeviceSet = serde_json::from_slice(&serialized_devices)?;
        assert_eq!(reloaded_devices.active_devices().len(), 2);
        assert_eq!(
            reloaded_devices
                .transparency_events()
                .iter()
                .map(|event| event.kind.as_str())
                .collect::<Vec<_>>(),
            vec!["device-added", "device-paired"]
        );

        let before_remove =
            alice_laptop.export_secret("room-two-device", "discrypt/v1/text", b"room", 32)?;
        let removed_phone = alice_laptop.remove_device("room-two-device", "alice", "phone")?;
        assert_eq!(removed_phone.state.epoch, added_phone.state.epoch + 1);
        assert!(device_set.remove_device(phone_leaf.device_id, removed_phone.state.epoch));
        assert_eq!(
            device_set
                .transparency_events()
                .last()
                .map(|event| event.kind.as_str()),
            Some("device-removed")
        );
        let after_remove =
            alice_laptop.export_secret("room-two-device", "discrypt/v1/text", b"room", 32)?;
        assert_ne!(before_remove, after_remove);
        assert_eq!(
            alice_laptop.member_labels("room-two-device")?,
            vec!["alice:laptop".to_owned()]
        );

        let removed_view = reloaded_phone.apply_external_remove_commit(
            "room-two-device",
            removed_phone.state.epoch,
            &removed_phone.commit,
            "alice:phone",
        )?;
        assert_eq!(removed_view.epoch, removed_phone.state.epoch);
        let bob_package = alice_laptop.generate_member_package(b"bob:laptop")?;
        let post_removal_add = alice_laptop.add_member_package("room-two-device", &bob_package)?;
        assert!(reloaded_phone
            .apply_external_commit(
                "room-two-device",
                post_removal_add.state.epoch,
                &post_removal_add.commit,
            )
            .is_err());

        let _ = std::fs::remove_file(alice_path);
        let _ = std::fs::remove_file(phone_path);
        Ok(())
    }

    #[test]
    fn openmls_join_from_welcome_validates_and_converges() -> Result<(), OpenMlsGroupError> {
        let alice_path = temp_path("alice-welcome");
        let bob_path = temp_path("bob-welcome");
        let mut alice = OpenMlsGroupEngine::open(&alice_path)?;
        let mut bob = OpenMlsGroupEngine::open(&bob_path)?;
        let created = alice.create_group("room-welcome", b"alice")?;
        let bob_package = bob.generate_member_package(b"bob")?;

        let added = alice.add_member_package("room-welcome", &bob_package)?;
        assert_eq!(added.state.epoch, created.epoch + 1);
        let welcome = added.welcome.as_ref().ok_or_else(|| {
            OpenMlsGroupError::OpenMls("OpenMLS add did not produce Bob welcome".to_owned())
        })?;
        let joined =
            bob.join_from_welcome("room-welcome", bob_package.signer_public_key(), welcome)?;
        assert_eq!(joined.epoch, added.state.epoch);
        assert_eq!(joined.confirmation_tag, added.state.confirmation_tag);
        let alice_export = alice.export_secret("room-welcome", "discrypt/v1/text", b"room", 32)?;
        let bob_export = bob.export_secret("room-welcome", "discrypt/v1/text", b"room", 32)?;
        assert_eq!(bob_export, alice_export);
        drop(bob);

        let mut reloaded_bob = OpenMlsGroupEngine::open(&bob_path)?;
        let reloaded_joined =
            reloaded_bob.load_group("room-welcome", bob_package.signer_public_key())?;
        assert_eq!(reloaded_joined.epoch, added.state.epoch);
        assert_eq!(
            reloaded_joined.confirmation_tag,
            added.state.confirmation_tag
        );
        assert_eq!(
            reloaded_bob.export_secret("room-welcome", "discrypt/v1/text", b"room", 32)?,
            alice_export
        );

        let charlie_package = reloaded_bob.generate_member_package(b"charlie")?;
        let charlie = alice.add_member_package("room-welcome", &charlie_package)?;
        let mut tampered = charlie.welcome.ok_or_else(|| {
            OpenMlsGroupError::OpenMls("OpenMLS add did not produce Charlie welcome".to_owned())
        })?;
        if let Some(last) = tampered.last_mut() {
            *last ^= 0x01;
        }
        assert!(reloaded_bob
            .join_from_welcome(
                "room-welcome",
                charlie_package.signer_public_key(),
                &tampered
            )
            .is_err());

        let _ = std::fs::remove_file(alice_path);
        let _ = std::fs::remove_file(bob_path);
        Ok(())
    }

    #[test]
    fn openmls_remove_commit_requires_expected_remove_target() -> Result<(), OpenMlsGroupError> {
        let alice_path = temp_path("remove-target-alice");
        let bob_path = temp_path("remove-target-bob");
        let mut alice = OpenMlsGroupEngine::open(&alice_path)?;
        let mut bob = OpenMlsGroupEngine::open(&bob_path)?;
        alice.create_group("room-remove-target", b"alice")?;
        let bob_package = bob.generate_member_package(b"bob")?;
        let bob_added = alice.add_member_package("room-remove-target", &bob_package)?;
        let bob_welcome = bob_added.welcome.as_ref().ok_or_else(|| {
            OpenMlsGroupError::OpenMls("OpenMLS add did not produce Bob welcome".to_owned())
        })?;
        bob.join_from_welcome(
            "room-remove-target",
            bob_package.signer_public_key(),
            bob_welcome,
        )?;

        let before_rejected_add =
            bob.export_secret("room-remove-target", "discrypt/v1/text", b"room", 32)?;
        let charlie_package = alice.generate_member_package(b"charlie")?;
        let charlie_added = alice.add_member_package("room-remove-target", &charlie_package)?;
        assert!(matches!(
            bob.apply_external_remove_commit(
                "room-remove-target",
                charlie_added.state.epoch,
                &charlie_added.commit,
                "bob",
            ),
            Err(OpenMlsGroupError::UnexpectedCommitMessage(_))
        ));
        assert_eq!(
            before_rejected_add,
            bob.export_secret("room-remove-target", "discrypt/v1/text", b"room", 32)?
        );

        let _ = std::fs::remove_file(alice_path);
        let _ = std::fs::remove_file(bob_path);
        Ok(())
    }

    #[test]
    fn openmls_rejects_stale_or_mismatched_pending_commit() -> Result<(), OpenMlsGroupError> {
        let path = temp_path("reject-commit");
        let mut engine = OpenMlsGroupEngine::open(&path)?;
        engine.create_group("room-reject", b"alice")?;
        let bob = engine.generate_member_package(b"bob")?;
        let commit = engine.stage_add_member("room-reject", &bob)?;

        assert!(matches!(
            engine.merge_pending_commit("room-reject", 0, &commit),
            Err(OpenMlsGroupError::StaleCommitEpoch {
                current: 0,
                attempted: 0
            })
        ));
        let mut tampered = commit.clone();
        if let Some(first) = tampered.first_mut() {
            *first ^= 0x01;
        }
        assert!(matches!(
            engine.merge_pending_commit("room-reject", 1, &tampered),
            Err(OpenMlsGroupError::CommitMismatch(1))
        ));

        let _ = std::fs::remove_file(path);
        Ok(())
    }
}
