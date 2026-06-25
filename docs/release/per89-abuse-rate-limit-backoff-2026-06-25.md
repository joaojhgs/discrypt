# PER-89 Abuse, Rate Limit, and Backoff Evidence

Issue: PER-89 / P10-T08.

## Evidence Level

This is local deterministic Rust abuse-control and transport-policy evidence. It is not production split-machine soak evidence, not a public-provider SLA claim, and not a full production-readiness claim.

## Implemented Behavior

- Invite flood attempts now have structured `AbuseDecision::RateLimited` results with a bounded retry delay.
- Reconnect and provider retry storms use bounded exponential `AbuseBackoffPolicy` decisions and fail closed with `retry_attempts_exhausted` after the configured attempt budget.
- Relay freeload accounting returns a structured `Deprioritized` decision while aggregate exported metrics remain content-free and omit actor/peer/group identifiers.
- Transport regression coverage ties the abuse soak to existing provider readiness classification, default provider retry policy validation, reconnect backoff behavior, and provider application relay-disabled errors.
- Providers remain signaling/rendezvous only. This task does not add any public-provider application message/media relay path.

## Files

- `crates/abuse/src/lib.rs`
- `crates/transport/Cargo.toml`
- `crates/transport/src/provider_adapters.rs`
- `.omx/plans/P10-T08-abuse-rate-limit-backoff-2026-06-25.md`
- `docs/release/per89-abuse-rate-limit-backoff-2026-06-25.md`

## Verification Commands

Planned local commands:

- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-abuse abuse_soak`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-abuse`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport abuse_rate_limit_backoff -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo test -p discrypt-transport --features mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter abuse_rate_limit_backoff -- --test-threads=1`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo fmt --check`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-abuse --lib -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-abuse --all-targets -- -D warnings`
- `RUSTUP_TOOLCHAIN=1.89.0 cargo clippy -p discrypt-transport --lib -- -D warnings`
- `git diff --check`

The captured verification transcript for this run is `target/e2e/per89-abuse-rate-limit-backoff.txt`.

## Known Gaps

- Public-provider rate-limit reproduction remains provider-dependent and is not claimed here.
- This does not implement diagnostics UI, package/release matrices, new relay-overlay runtime forwarding, or broader Phase 10 roadmap work.
