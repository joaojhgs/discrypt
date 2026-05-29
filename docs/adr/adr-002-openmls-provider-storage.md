# ADR-002: OpenMLS provider, persistent storage, exporters, and repair integration

Status: accepted  
Date: 2026-05-29

## Context

Discrypt needs one MLS implementation path for group state, commit processing, Welcome admission, exporter handling, and fork repair. The launch checklist requires the selected OpenMLS version/provider, persistent group-store design, exporter handling rules, MLS commit/Welcome persistence, and integration with deterministic fork repair.

## Decision

Discrypt uses the upstream OpenMLS stack through `crates/mls-core/src/openmls_engine.rs`:

| Dimension | Decision |
| --- | --- |
| OpenMLS crate | `openmls = 0.8.1` from the workspace lockfile |
| Crypto provider | `openmls_rust_crypto::RustCrypto` |
| Persistent provider | `openmls_sqlite_storage::SqliteStorageProvider<JsonOpenMlsCodec, Connection>` |
| Storage codec | `JsonOpenMlsCodec` using `serde_json` for OpenMLS provider records |
| Ciphersuite | `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519` |
| Signature scheme | `ED25519` |
| Group store | `OpenMlsGroupEngine` owns live `MlsGroup` values and reloads by `MlsGroup::load` from a per-profile SQLite provider path |
| Exporters | `OpenMlsGroupEngine::export_secret` feeds Rust service boundaries only; approved service labels are `Text`, `Media`, and `ContentKey` in `crates/mls-core/src/exporter.rs` |
| Commit persistence | add/remove/stage operations keep pending commit bytes, compare delivered bytes before merge, then merge through OpenMLS storage |
| Welcome persistence | add-member operations serialize Welcome and GroupInfo bytes for authorized joiners; joiners validate expected group id before accepting |
| Fork repair | `crates/mls-delivery/src/lib.rs` detects replay, downgrade, and divergent epoch summaries; repair plans re-add/rejoin through OpenMLS instead of replaying divergent commits |

## Exporter boundary

Raw OpenMLS exporter output is never returned to React, Tauri command payloads, signaling, relays, logs, crash reports, or UI state. Rust services request exporter-derived material through explicit labels and service-owned context. Media derives SFrame keys from the `Media` label, text derives message keys from the `Text` label, and retention/content-key flows derive from the `ContentKey` label.

## Persistence and recovery behavior

- The OpenMLS provider SQLite path is separate from app UI state and is opened by `DiscryptOpenMlsProvider::open`.
- Provider migrations run before use.
- `OpenMlsGroupSnapshot` records group id, epoch, confirmation tag, pending proposal count, and pending commit state for service/UI status surfaces without exposing secrets.
- Pending commit bytes must match before merge, which prevents accepting a different delivered commit for the expected epoch.
- Welcome joins validate the expected group id and persist the joined group through the selected provider.
- Fork repair starts from the last common accepted epoch summary and re-adds/rejoins losing members through fresh OpenMLS state; invalid divergent commits are not replayed.

## Evidence

- `crates/mls-core/src/provider.rs` — `OpenMlsProviderDecision` and selected version/provider metadata.
- `crates/mls-core/src/openmls_engine.rs` — RustCrypto provider, SQLite provider, group create/add/remove/join/reload/export behavior.
- `crates/mls-core/src/exporter.rs` — approved Rust-only exporter labels and domain separation.
- `crates/mls-delivery/src/lib.rs` — fork/replay/downgrade detection and explicit repair plans.
- `Cargo.toml` and `crates/mls-core/Cargo.toml` — selected OpenMLS dependency versions.
