# G009 security, privacy, and no-shim gate

This gate is the release-contract for Discrypt privacy boundaries before the final
production and two-user Tauri E2E goals. It is intentionally fail-closed: if a
path cannot prove it is using backend-owned encrypted transport and redacted
observability, the UI/docs must say so instead of displaying production labels.

## Forbidden in provider/log/debug/persistence boundaries

Provider payloads, logs, IPC diagnostic rows, debug drawers, screenshots, and
release docs must not expose:

- raw SDP offers/answers or ICE candidates/credentials;
- TURN usernames, credentials, or credential-bearing URLs;
- raw room seeds, invite room secrets, rendezvous topics, group names, channel
  names, display names, or device names as provider routing identifiers;
- plaintext text messages, audio frames, MLS/SFrame/content keys, or recovery
  material;
- fake participants, shim transports, manual pairing controls, or "production
  ready" labels without backend proof.

## Required implementation posture

- Signaling adapters exchange only `OpaqueSignalingPayload` and sealed WebRTC
  negotiation/control frames. `crates/transport/src/signaling.rs` is the
  adapter contract and explicitly states that raw SDP, ICE credentials, TURN
  secrets, names, messages, audio, and keys are outside the provider boundary.
- Debug output for sensitive transport structs is redacted: SDP/ICE wrappers,
  sealed negotiation payloads, opaque signaling payloads, and TURN server
  credentials must not derive raw `Debug`.
- Runtime observability must use hashed/redacted references such as
  `redacted_observable_ref`, `redacted_endpoint_label`, or
  `redacted_observable_label`, never raw rendezvous topics or message ids.
- Production Tauri storage uses `EncryptedAppDb` with OS keychain wrapping.
  Plain `FileAppStore` and browser `localStorage` are test/local-dev harnesses
  only and must stay labeled as non-production.
- Local-dev fallback may persist UI state to `discrypt.local-dev.app-state.v1`
  only when the explicit local-dev/test harness path is active; native packaged
  builds must use Tauri IPC and the Rust storage boundary.
- Dependency/security/license gates remain part of G009 evidence:
  `test:cargo-audit-g122`, `test:npm-audit-g123`, and `test:cargo-deny-g121`.

## Verification commands

```bash
npm --prefix apps/ui run test:security-privacy-g009
npm --prefix apps/ui run test:honesty
npm --prefix apps/ui run test:provider-metadata-capture-g133
npm --prefix apps/ui run test:stun-turn-provider-privacy-g132
npm --prefix apps/ui run test:cargo-audit-g122
npm --prefix apps/ui run test:npm-audit-g123
npm --prefix apps/ui run test:cargo-deny-g121
```

G009 does **not** by itself claim final production readiness or final two-user
Tauri voice/text E2E. Those are G011 and G012.
