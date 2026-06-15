# Discrypt Release Gap Matrix - 2026-06-15

## Purpose

This matrix is the current release truth source for the Phase 0 reset. It
supersedes older "complete", "green", or production-ready ledgers unless a row
below names fresh evidence after the June 2026 regression reset.

This document is not production-ready evidence. It is a gap matrix and blocker
index for follow-up implementation and verification.

## Final report verdict

Final report verdict: not production-ready.

Blockers exist: yes.

Reason: protected group admission, presence, WebRTC route evidence, storage
recovery, voice/media proof, installed two-user E2E, packaging, and security
release gates still require fresh evidence.

## Status vocabulary

Rows use exactly one current status:

- `verified` - Fresh post-reset evidence proves this row's limited claim.
- `implemented-unverified` - Code or harness support appears present, but the
  required release proof is missing or weaker than the claim.
- `planned` - The row is defined in plans/docs but not implemented or not
  observable as a release feature.
- `blocked` - A known regression, missing proof, or unsafe claim boundary blocks
  release promotion.

## Release gap matrix

| Feature / gate | Current status | Fresh evidence boundary | Missing or blocking work |
| --- | --- | --- | --- |
| Release definitions for production-ready, E2E-tested, split-machine, voice proof, and overlay relay | verified | `docs/release/release-verification-matrix.md` defines the frozen terms and `scripts/check-release-verification-matrix.mjs` gates key wording. | Keep future docs tied to these definitions; do not use weaker harness rows as stronger proof. |
| Current known-bad regression ledger | verified | `docs/release/current-regressions.md` lists `REG-INVITE-BROKEN-GROUP`, `REG-MANUAL-ADMISSION-INVISIBLE`, `REG-PRESENCE-OFFLINE`, `REG-WEBRTC-ICE-STATE-NEW`, and `REG-STORAGE-VAULT-REINSTALL-FAILURE`. | Each regression still needs an implementation/test task before release promotion. |
| Phase 0 worktree and stale-claim reset | implemented-unverified | Phase 0 plan artifacts exist under `.omx/plans/`; this matrix adds the current truth source. | PR/QA/architect review must accept the reset before downstream tasks rely on it. |
| Identity setup and local profile persistence | implemented-unverified | Prior command and release docs describe local setup/persistence paths. | Needs fresh Tauri/backend evidence on the current branch, including no silent profile replacement on unreadable state. |
| Storage vault/keyring reinstall recovery | blocked | `REG-STORAGE-VAULT-REINSTALL-FAILURE` records the current known-bad user report. | Add reinstall/recovery regression coverage for existing profile dirs, missing/wrong credentials, keyring-unavailable mode, explicit errors, and no-overwrite assertions. |
| Invite parsing and protected group membership | blocked | `REG-INVITE-BROKEN-GROUP` records that invite parsing/display can outpace usable protected membership. | Prove authorized MLS Welcome/add, persisted OpenMLS group state, channel visibility, and restart survival before any joined claim. |
| Manual admission visibility and owner/requester state | blocked | `REG-MANUAL-ADMISSION-INVISIBLE` records ambiguous pending/admitted/rejected state. | Backend-governed admission state must drive UI rows for pending, approved, rejected, failed, and persisted reload states. |
| OpenMLS-backed group state and revocation authority | implemented-unverified | Original plan and release docs define OpenMLS and governance invariants. | Needs current tests proving membership, revocation/kick, future send/receive rejection, and relay authority rejection after removal. |
| Backend-governed roles and staff/member policy | implemented-unverified | Governance/admission plans describe owner/admin/member policy expectations. | Needs signed/backend-governed role evidence; frontend labels alone are not sufficient. |
| Presence and online state | blocked | `REG-PRESENCE-OFFLINE` records current unreliable/offline presence behavior. | Prove online, offline, stale, and unknown states with backend/provider source evidence and timestamps; do not synthesize online state in UI. |
| Provider signaling adapters: MQTT, Nostr, IPFS PubSub, and Discrypt QUIC rendezvous | implemented-unverified | Release docs and adapter readiness docs define signaling-only boundaries and optional public rows. | Public/provider evidence must remain signaling-only and must not relay application messages or media. Missing external credentials/endpoints must be explicit skips. |
| WebRTC text/control route establishment | blocked | `REG-WEBRTC-ICE-STATE-NEW` records unresolved ICE route state. | Prove direct/STUN, configured TURN, or approved encrypted peer-overlay route evidence before connected/delivered claims. |
| Application text delivery through backend-owned runtime | implemented-unverified | Prior release docs name local command/harness proofs and public MQTT/Nostr opt-in tests. | Needs two live profiles or installed app evidence for protected message delivery and receipts without provider application relay. |
| Native voice/media proof | planned | Release definitions describe what voice proof requires. | Need real capture/transport/remote receive or loopback artifacts, route evidence, mute/speaking/volume/leave checks, and cleanup assertions. |
| STUN/TURN policy and fail-closed fallback | implemented-unverified | G008/G132 release rows describe deterministic local gates and opt-in credentialed TURN. | Credentialed TURN success across constrained networks remains opt-in/missing; relay-only paths must fail closed without route evidence. |
| Encrypted peer-assisted overlay relay | planned | Original plan locks STUN -> peer relay overlay -> TURN and release definitions define overlay relay evidence. | Overlay route evidence, relay authority, ciphertext-only validation, and fail-closed behavior are not production-proven. |
| Local-dev fallback and UI honesty | implemented-unverified | Existing release gates distinguish local fallback from production proof. | Continue hiding fallback-only/debug states from production UX and ensure UI claims are backed by backend/OpenMLS/transport evidence. |
| Two-user installed Tauri text plus voice E2E | planned | G012 docs define the final installed-app proof boundary. | Need retained artifacts for two separately installed/running app profiles completing setup, protected text both ways, voice actions, and persistence reload. |
| Packaging, SBOM, reproducibility, and platform runners | implemented-unverified | Release matrix rows list Linux, macOS, Windows, Android, SBOM, reproducibility, and package smoke gates. | Need fresh runner/package artifacts tied to the release candidate commit before production promotion. |
| Security/privacy/no-shim release gates | implemented-unverified | Security docs and release rows define no-placeholder, no-fallback, pcap, provider metadata, advisory, and dependency gates. | Fresh gate outputs must be retained; release copy must stay content-private and metadata-minimizing, not metadata-anonymous. |
| Final production readiness claim | blocked | This matrix is the current final report and says not production-ready. | All blocked, planned, and implemented-unverified rows must be resolved with fresh evidence before any final report may say production-ready. |

## Stale ledger handling

Older docs such as `docs/release/production-gap-matrix-2026-06-01.md`,
`docs/release/g011-production-readiness-matrix.md`, historical handoffs, and
green ultragoal ledgers remain context only. They do not supersede this matrix
and do not prove production readiness after the June 2026 reset unless the
specific evidence is re-run and cited in a current row.

## Known blocker mapping

| Blocker ID | Matrix row | Required next proof |
| --- | --- | --- |
| REG-INVITE-BROKEN-GROUP | Invite parsing and protected group membership | Two-profile invite/admission/OpenMLS persistence regression. |
| REG-MANUAL-ADMISSION-INVISIBLE | Manual admission visibility and owner/requester state | Backend admission status UI and reload regression. |
| REG-PRESENCE-OFFLINE | Presence and online state | Backend/provider-backed presence source evidence regression. |
| REG-WEBRTC-ICE-STATE-NEW | WebRTC text/control route establishment | ICE state and route evidence regression with fail-closed reporting. |
| REG-STORAGE-VAULT-REINSTALL-FAILURE | Storage vault/keyring reinstall recovery | Reinstall/recovery no-overwrite regression. |

## Release handling rules

- Do not claim joined, connected, online, admitted, delivered, voice-active, or
  persisted unless the backend/OpenMLS/transport/storage evidence for that row
  is fresh and cited.
- Do not treat invite parsing as membership.
- Do not treat MQTT, Nostr, IPFS PubSub, or Discrypt QUIC rendezvous as
  application-message or media relays.
- Do not treat local-dev fallback, browser Playwright, same-process harnesses,
  or docs-only checks as installed-app production proof.
- Do not replace an unreadable vault/keyring profile with a new profile.
- Do not promote the final report to production-ready while any row remains
  `blocked`, `planned`, or `implemented-unverified`.

