#!/usr/bin/env node
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const read = (path) => readFileSync(resolve(repoRoot, path), "utf8");
const files = {
  review: read("docs/security/g095-independent-security-review.md"),
  sframe: read("crates/media/src/sframe.rs"),
  transform: read("crates/media/src/transform_bridge.rs"),
  openmls: read("crates/mls-core/src/openmls_engine.rs"),
  governance: read("crates/mls-core/src/governance.rs"),
  admission: read("crates/admission/src/lib.rs"),
  content: read("crates/content-keys/src/lib.rs"),
  delivery: read("crates/mls-delivery/src/lib.rs"),
  appdb: read("crates/storage/src/appdb.rs"),
  storage: read("crates/storage/src/lib.rs"),
};

const failures = [];
function requireToken(file, token) {
  if (!files[file].includes(token)) failures.push(`${file} missing token: ${token}`);
}
function requireRegex(file, regex, message) {
  if (!regex.test(files[file])) failures.push(`${file} missing pattern: ${message ?? regex}`);
}

for (const token of [
  "# G095 independent security review",
  "OpenMLS integration",
  "SFrame/RFC 9605 framing",
  "KID sender binding",
  "Content-key lifecycle",
  "Admission/password flow",
  "Governance signatures",
  "Storage encryption",
  "not a full RFC 9605 implementation yet",
  "does not certify wire-format parity",
  "No raw-key boundary",
  "Required verification commands",
  "bounded acceptance",
]) requireToken("review", token);

for (const badClaim of [
  /is full RFC[- ]?9605 (compliant|certified|complete)/i,
  /formally verified/i,
  /production[- ]certified/i,
]) {
  if (badClaim.test(files.review)) failures.push(`review contains overclaim: ${badClaim}`);
}

for (const token of [
  "full RFC 9605 implementation yet",
  "pub struct SenderBinding",
  "KidBindingMismatch",
  "pub struct SFrameKey",
  "impl Drop for SFrameKey",
  "zeroize",
  "pub struct ReplayWindow",
  "rotates_media_kid_and_key_on_mls_epoch_membership_change",
  "rejects_kid_not_derived_from_mls_epoch_sender_binding",
]) requireToken("sframe", token);
for (const token of ["encoded frame", "ProtectedFrame", "RustTransformBridge", "Raw media keys stay"]) requireToken("transform", token);

for (const token of [
  "openmls_rust_crypto::RustCrypto",
  "openmls_sqlite_storage",
  "SqliteStorageProvider",
  "MlsGroup::load",
  "export_secret",
  "join_from_welcome",
  "WelcomeGroupIdMismatch",
  "openmls_join_from_welcome_validates_and_converges",
  "openmls_rejects_stale_or_mismatched_pending_commit",
]) requireToken("openmls", token);

for (const token of [
  "Ed25519",
  "pub fn signed_by",
  "verify_signature",
  "InvalidSignature",
  "Unauthorized",
  "EvictedCommitter",
  "real_device_signature_rejects_tampering_and_key_swaps",
  "enforces_role_retention_invite_ban_and_device_authority",
]) requireToken("governance", token);

for (const token of [
  "InviteSignalingMetadata",
  "ice_endpoint_policy",
  "verify_issuer_signature",
  "InviteEndpointPolicy::ProductionTls",
  "OfflineVerifierRejected",
  "OpaquePake",
  "OnlineAuthorizedHelper",
  "AuthorizedHelperProof",
  "OnlineAdmissionHelper",
  "AuthorizedWelcome",
  "finalize_helper_admission",
  "invite_descriptor_signs_signaling_metadata_and_rejects_invalid_values",
  "online_helper_flow_rate_limits_and_signs_expiring_proofs",
  "admission_rejects_offline_verifier_and_requires_welcome",
]) requireToken("admission", token);

for (const token of [
  "derive_content_key",
  "LiveKeyOracle",
  "MembershipProof",
  "request_key",
  "CrossDeviceShredState",
  "retention",
  "live_key_oracle_gates_membership_and_rate_limits_with_decoys",
]) requireToken("content", token);
for (const token of [
  "TextRetentionMetadata",
  "retention_allows_decrypt",
  "retention policy requires live key before plaintext render",
]) requireToken("delivery", token);

for (const token of [
  "Aes256Gcm",
  "EncryptedAppDbEnvelope",
  "wrapped_data_key",
  "data_key.zeroize",
  "production-storage",
  "MemoryAppDbKeychain",
  "LinuxOsKeychain",
  "encrypted_app_db_persists_wrapped_key_separately_from_keychain",
]) requireToken("appdb", token);
for (const token of [
  "content keys are intentionally not accepted",
  "Recover account continuity without restoring archival content keys",
  "zeroize",
]) requireToken("storage", token);

requireRegex("appdb", /#\[cfg\(all\(target_os = "linux", feature = "production-storage"\)\)\][\s\S]*pub struct LinuxOsKeychain/, "LinuxOsKeychain is cfg-gated to Linux production-storage");
requireRegex("appdb", /#\[cfg\([\s\S]*not\(feature = "production-storage"\)[\s\S]*\)\][\s\S]*pub struct MemoryAppDbKeychain/, "MemoryAppDbKeychain is excluded from production-storage builds");

if (/TODO|FIXME|unimplemented!|todo!/i.test(files.review)) {
  failures.push("review contains unfinished-work marker");
}

if (failures.length > 0) {
  console.error("G095 security review check failed:");
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log("G095 security review check passed");
