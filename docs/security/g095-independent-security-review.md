# G095 independent security review

This review covers the Phase N security-review slice for the Discrypt production E2E P2P overlay mesh plan. It is a source-backed review of the current repository state, not a release attestation. Each domain below names the code boundary, the evidence already present, the release-risk decision, and the follow-up gates that must stay green in later Phase N goals.

## Review boundary and decision rules

- **Scope:** OpenMLS integration, SFrame/RFC 9605 media framing status, KID sender binding, content-key lifecycle, admission/password flow, governance signatures, and storage encryption.
- **Decision rule:** a domain is accepted for this slice only when the implementation is backed by executable tests or a static gate and the review identifies any remaining release claim that would be too broad.
- **Important SFrame claim limit:** `crates/media/src/sframe.rs` implements a Rust-owned SFrame-like AEAD frame boundary and explicitly says it is not a full RFC 9605 implementation yet. This review accepts the current sender-binding and relay-blindness contracts, but it does not certify wire-format parity with the RFC.
- **No raw-key boundary:** MLS exporter material, SFrame keys, content keys, data-encryption keys, and recovery secrets must remain in Rust/keychain-owned boundaries and must not be exposed as React/Tauri command payloads.

## Findings matrix

| Domain | Current implementation evidence | Current release decision | Required follow-up gate |
| --- | --- | --- | --- |
| OpenMLS integration | `crates/mls-core/src/openmls_engine.rs` uses `openmls_rust_crypto::RustCrypto` and `openmls_sqlite_storage::SqliteStorageProvider`, creates groups, stages/merges add and remove commits, joins from `Welcome`, reloads persisted groups with `MlsGroup::load`, and exports scoped secrets through `export_secret`. | Accepted for the G095 review boundary as a real OpenMLS provider/storage path. Pending-commit mismatch, stale epoch, Welcome group mismatch, add/remove rekey, and reload behavior are tested. | Keep `cargo test -p discrypt-mls-core openmls --quiet` green, then later Phase N pcap/malicious-peer gates must run multi-process flows with this same engine. |
| SFrame/RFC 9605 framing | `crates/media/src/sframe.rs` provides `SenderBinding`, `SFrameKey`, AEAD protection/opening, authenticated binding context, per-KID `ReplayWindow`, and tests for tamper, duplicate KID, replay, and epoch rotation. `crates/media/src/transform_bridge.rs` keeps JavaScript on encoded bytes and opaque metadata only. | Accepted only as a compact SFrame-like protected-frame contract. The code and this review both reject a full RFC 9605 compliance claim until vectors and wire-format interop are added. | Keep `cargo test -p discrypt-media --quiet` green; G096/G097 must add network capture evidence showing relay/TURN paths contain protected frame bytes only. |
| KID sender binding | `SenderBinding::derive_for_epoch` derives KID from group id, epoch, MLS leaf index, device id, and MLS epoch/exporter secret; `verify_derived_kid` fails closed with `KidBindingMismatch`; rotation tests prove old/new epoch KIDs diverge. | Accepted for malicious relay and same-member device-binding tests. Receiver identity still depends on the MLS/governance layer delivering the valid binding registry. | Later adversarial tests must include forged KID, stale epoch, removed-device, and cross-member impersonation across the full app-service path. |
| Content-key lifecycle and live-key oracle | `crates/content-keys/src/lib.rs` derives content keys from MLS exporter material, enforces retention lock/shred state, implements cross-device shred propagation, and limits archival live-key requests with `LiveKeyOracle`. `crates/mls-delivery/src/lib.rs` authenticates retention metadata in message envelopes and renders locked state when retention blocks local decrypt. | Accepted for lifecycle invariants in this slice: key derivation is MLS-exporter scoped, retention metadata is authenticated, and live-key responses are signed membership/rate limited. This is not a guarantee against recipient screenshots, modified clients, or plaintext already exported before shred. | Keep content-key, delivery, retention, and storage tests green; later Phase N retention tests must prove removed members and shredded messages cannot fetch future keys. |
| Admission/password flow | `crates/admission/src/lib.rs` issues opaque signed invite descriptors, signs signaling metadata and ICE endpoint policy, validates production TLS vs loopback-dev endpoints, rejects offline verifier gates, supports OPAQUE/PAKE-shaped gates, uses online helper proofs with Ed25519 signatures and rate limits, and requires exact `AuthorizedWelcome` payload authorization for final admission. | Accepted for current invite and admission contract. Invite links must carry signed rendezvous/ICE policy and must not be treated as bare incremental ids. | Keep `cargo test -p discrypt-admission --quiet` green; later full-app tests must prove UI-created invites include signed signaling/TURN metadata and final Welcome authorization. |
| Governance signatures | `crates/mls-core/src/governance.rs` signs domain-separated canonical events with Ed25519, verifies signatures before state transitions, applies authority checks, handles deterministic same-epoch ordering, and rejects evicted committers. | Accepted for signed governance-event primitives and deterministic conflict handling. Production call sites must use `GovernanceEvent::signed_by` with the real local device key. | Keep `cargo test -p discrypt-mls-core governance --quiet` green; later review must trace UI/admin actions into signed governance events and MLS epoch changes. |
| Storage encryption | `crates/storage/src/appdb.rs` wraps the app database in an `EncryptedAppDbEnvelope`, uses AES-256-GCM for the data key and data payload, zeroizes transient data keys, keeps deterministic memory keychains out of `production-storage`, and exposes a Linux Secret Service keychain under `LinuxOsKeychain`. `crates/storage/src/lib.rs` excludes content keys from sealed account-continuity backup APIs. | Accepted for keychain-wrapped app-state persistence on the Linux production-storage path and for test harness parity with explicit cfg separation. Platform keychain implementations for every release target must be verified before cross-platform release. | Keep `cargo test -p discrypt-storage --quiet` green; packaging gates must build production-storage features on each release target with no fallback keychain. |

## Cross-domain risks that remain outside this review slice

1. **Full RFC 9605 parity:** current media frames protect the same security-critical boundary for Discrypt harnesses, but full SFrame wire-format vectors and interop are a later hardening gate.
2. **End-to-end app-service evidence:** many primitives are tested in Rust crates; later goals must keep wiring them through the Tauri app service and UI without swapping in local-only state paths.
3. **Multi-process adversaries:** later Phase N goals must run malicious relay, replay, drop, stale Welcome, forged KID, revoked invite, removed member, and pcap/log leakage scenarios across actual networked processes.
4. **Platform keychain matrix:** Linux `production-storage` has an OS keychain path; macOS, Windows, and Android packaging must prove their own secure keychain bindings before release tags.
5. **Operational secrets and logs:** release verification must continue proving signaling, relay, crash, and package logs do not include SDP, ICE credentials, raw messages, SFrame keys, MLS secrets, or database rows.

## Required verification commands for this slice

```sh
npm --prefix apps/ui run test:security-review-g095
cargo test -p discrypt-media --quiet
cargo test -p discrypt-mls-core openmls --quiet
cargo test -p discrypt-mls-core governance --quiet
cargo test -p discrypt-admission --quiet
cargo test -p discrypt-storage --quiet
npm --prefix apps/ui run test:honesty
npm --prefix apps/ui run test:command-coverage
npm --prefix apps/ui run build
cargo fmt --all --check
cargo check --workspace --quiet
cargo clippy --workspace --all-targets --quiet -- -D warnings
git diff --check
```

## Review outcome

G095 is acceptable when the static review gate and all commands above pass. The review outcome is **bounded acceptance of the current security-critical contracts**, with explicit non-acceptance of unproven full RFC 9605 parity and unproven cross-platform keychain coverage.
