# G039 invite signaling metadata review

This review records the current implementation state for the G039 invite
metadata story. It is intentionally an honesty artifact, not a production
completion claim.

## Required production contract

Production invite descriptors must carry all joiner-critical rendezvous metadata
under the same signed canonical descriptor as the opaque invite id and admission
commitment:

- opaque invite id, not derived from group names, counters, or topology;
- room-secret commitment only, never the raw room secret in stored state or UI
  state;
- signaling endpoint and endpoint policy that a joiner can validate before
  contacting infrastructure;
- trust metadata/fingerprint for the signaling endpoint or admission helper;
- expiry, revocation/max-use posture, and issuer signature coverage over the
  full descriptor;
- redacted command/UI state that does not expose room secrets, identity bytes,
  group topology, media bytes, MLS key material, or message content.

## Current implementation map

- `crates/admission/src/lib.rs`
  - `StoredInvite` provides opaque random invite ids, domain-separated room-secret
    commitments, issuer keys/signatures, expiry, max-use, consumed-use accounting,
    and revocation event ids.
  - Canonical signing currently covers invite id, room-secret commitment, issuer
    public key, expiry, and max-use.
- `external/signaling-repository/src/client.rs`
  - `SignalingClientConfig` validates a public endpoint string for supported
    schemes and keeps client token/nonce seed requirements explicit.
- `apps/desktop/src-tauri/src/lib.rs`
  - `InviteView` and `create_invite` still expose a room-secret-derived query
    parameter shape and only a room-secret hash in state.
- `apps/ui/src/commands.ts` and `apps/ui/src/main.tsx`
  - the TypeScript command fallback and invite card mirror the same
    room-secret-hash-only state shape.

## Review findings

| Severity | Finding | Evidence | Required resolution |
| --- | --- | --- | --- |
| High | Invite descriptor does not yet carry signaling endpoint, endpoint policy, or trust fingerprint. | `StoredInvite` fields stop at invite id, room-secret commitment, issuer key/signature, expiry, max-use, use count, and revocation id. | Add explicit descriptor fields for endpoint URL, endpoint policy, and trust metadata/fingerprint. |
| High | Canonical signature coverage excludes future endpoint/trust metadata. | `StoredInvite::signing_bytes` only serializes invite id, room-secret commitment, issuer key, expiry, and max-use. | Include every production descriptor field in the domain-separated canonical signing bytes and add tamper tests per field. |
| High | Desktop invite links are not production descriptor links. | `create_invite` formats `discrypt://join/v1/{invite_key}?room_secret=...&exp=...&max=...`. | Replace room-secret query links with a signed descriptor/link format that carries endpoint and trust metadata without raw secrets. |
| Medium | Join/parse state cannot surface endpoint or trust posture. | `JoinGroupRequest` has only `invite_code` and optional `group_name`; `InviteView` has no endpoint or trust fields. | Extend command state with redacted endpoint/trust status and parse validation results. |
| Medium | UI copy still emphasizes room-secret hash instead of endpoint/trust status. | The latest-invite card renders `Room secret hash`. | Show signaling endpoint/trust status honestly and keep the commitment redacted or diagnostic-only. |

## Documentation and release gate notes

- Existing Phase 5 documentation remains accurate for admission controls, but it
  should not be read as claiming complete production invite signaling metadata.
- Existing Phase 6 documentation remains accurate for content-blind signaling and
  metadata-minimizing infrastructure, but it does not yet describe invite-carried
  endpoint/trust metadata.
- G039 should not be marked production-complete until descriptor tests prove:
  canonical signature coverage for endpoint/trust/policy fields; invalid
  endpoint/trust rejection; redaction/no-secret leakage in desktop and UI state;
  and end-to-end create/join propagation of endpoint/trust metadata.
