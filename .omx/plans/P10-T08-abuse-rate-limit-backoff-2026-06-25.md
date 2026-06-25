# P10-T08 Abuse, Rate Limit, and Backoff Plan

Source: PER-89 / P10-T08, Phase 10 signaling adapters, public profiles, and abuse/privacy.

## Requirements Summary

- Implement focused backend/core abuse handling for invite flood, reconnect storm, relay freeload, and provider rate-limit/backoff behavior.
- Preserve the release invariants from the issue body: provider adapters remain signaling/rendezvous only, invite parsing is not membership, and relay/application delivery must fail closed unless backed by WebRTC/TURN/peer-overlay route evidence.
- Use `.omc/plans/discrypt-plan.md` AC-ABUSE and `docs/phase-5-governance-admission-recovery-abuse.md` as prior local context; `.omx/plans/production-release-master-plan-2026-06-10.md` is not present in this checkout, so the issue body is the Phase 10 task source.

## Code Paths

- `crates/abuse/src/lib.rs`: add typed abuse decisions and deterministic soak harness coverage.
- `crates/transport/src/provider_adapters.rs`, `crates/transport/src/policy.rs`, `crates/transport/src/session.rs`: use existing provider rate-limit classification, bounded provider backoff, provider relay-disabled paths, and reconnect backoff in transport-side regression coverage.
- `docs/release/per89-abuse-rate-limit-backoff-2026-06-25.md`: record evidence level and caveats.

## Acceptance Criteria

- Invite flood is rate-limited with structured evidence while allowed invite attempts remain possible after the window.
- Reconnect storm attempts receive monotonic bounded backoff and exhaust fail-closed instead of reporting connected state.
- Relay freeloaders are deprioritized by contribution deficit, without exporting peer ids in aggregate metrics.
- Provider rate-limit failures map to typed `provider_rate_limited` health/readiness and use bounded retry/backoff policy.
- Public providers still cannot carry application control/message/media relay payloads.
- Verification artifacts clearly state this is local deterministic Rust evidence, not production split-machine soak or full release readiness.

## Implementation Steps

1. Extend `discrypt-abuse` with small typed decision structs for rate-limit/backoff/freeload outcomes and a `run_abuse_soak` harness that covers the four required scenarios.
2. Add targeted unit tests in `crates/abuse` for the harness, metrics redaction, and fail-closed decisions.
3. Add `discrypt-abuse` as a transport dependency and a transport regression test that combines existing provider readiness classification, `ProviderRetryBackoffPolicy`, `ReconnectBackoffPolicy`, and provider relay-disabled behavior.
4. Write release evidence under `docs/release` and capture command output under `target/e2e/per89-abuse-rate-limit-backoff.txt`.
5. Run focused Rust tests, fmt, clippy where feasible, and `git diff --check`; commit and open a draft PR before QA handoff.

## Risks and Mitigations

- Risk: duplicating existing transport policy logic. Mitigation: keep the new abuse crate layer deterministic and transport tests reference existing public APIs.
- Risk: overstating production readiness. Mitigation: release doc and QA handoff label evidence as local/harness only.
- Risk: provider fallback accidentally becomes application relay. Mitigation: tests assert `broadcast_control` and `take_control_payloads` fail with the relay-disabled message.

## Verification

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-abuse abuse_soak`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-abuse`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport abuse_rate_limit_backoff -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter abuse_rate_limit_backoff -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-abuse --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --lib -- -D warnings`
- `git diff --check`
