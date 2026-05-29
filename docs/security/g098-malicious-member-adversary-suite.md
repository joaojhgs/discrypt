# G098 malicious member/device adversary suite

This gate adds deterministic Phase N coverage for malicious members and devices on top of the existing media, text, MLS delivery, and governance harnesses.

## Covered adversary cases

- **Media impersonation**: a protected frame produced by Alice cannot be relabeled with Mallory's MLS leaf/device KID; the receiver rejects the frame during media authentication.
- **Evicted member text send**: a signed text envelope from a removed leaf is rejected by the current epoch authorized-sender set before content is rendered or persisted.
- **Evicted device media send**: after device rotation, the receiver registry contains only the replacement epoch/device binding; frames from the removed device KID are rejected.
- **Forked MLS commit**: a same-epoch divergent commit is rejected as divergent state, not silently merged or replayed.
- **Out-of-epoch governance**: governance events signed for the wrong epoch fail closed.
- **Unauthorized governance**: non-admin/non-owner leaves cannot revoke invites or mutate group policy.
- **Removed admin race**: a banned admin cannot win a same-epoch race against the owner removal event.

## Harness location

- `harness/multinode/src/lib.rs` exposes `MaliciousMemberAdversarySmoke` and `malicious_member_adversary_smoke`.
- The regression test is `malicious_member_adversary_smoke_covers_impersonation_eviction_divergence_and_admin_cases`.
- The gate reuses real crate seams: `discrypt-media` SFrame/KID binding, `discrypt-mls-delivery` text receive and fork detection, and `discrypt-mls-core` governance authority checks.

## Boundary

This is a headless deterministic adversary harness. It proves the member/device rejection semantics at the Rust crate boundaries used by the app. It does not replace the later full multi-device release E2E gate with live network processes, packet captures, and platform packaging.
