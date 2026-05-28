# discrypt — v1 Implementation Work Plan

> **Status: PENDING APPROVAL** (v1.3 — consensus re-validated 2026-05-24: **Architect SOUND-WITH-CHANGES** + **Critic APPROVE-WITH-MERGE-LIST**, all merge items applied below).
> **v1.2 changes:** Web **removed from v1** (deferred to a later track); **multi-device identity is now v1**; retention gains presets + warned-unlimited opt-in + shorten-retro/lengthen-future semantics; added a precise **Threat Model**, **governance log**, **MLS delivery/fork-recovery layer**, **admission hardening**, **identity/recovery**, and **abuse** stories.
> **v1.3 changes (consensus):** closed the blockers — a single **canonical ordering comparator** for MLS delivery/governance/device-eviction; an **AC-MLS-FORK recovery oracle**; honest **cross-device shred** + **account-continuity recovery** model; scoping fixes for AC-PRESENCE / AC12 / AC9. See §11.
> Source spec: `/home/developer/projects/discrypt/.omc/specs/deep-interview-discrypt.md` (authoritative).
> Mode: **deliberate** (high-risk distributed-systems + applied-crypto greenfield).
> Crypto stack is **LOCKED**: OpenMLS + crypto-shredding + SFrame. STUN → peer-relay overlay → TURN.

---

## 0. Threat Model & Security Guarantees (NEW v1.2 — reviewer's #1 ask)

Precise guarantees, not absolutes. All product/UX copy must align with this section.

**Adversary classes:**
1. **Passive infrastructure** (signaling, STUN, TURN, push, network) — sees IPs, timing, topology, push tokens. *Guarantee:* no content, no content keys, no durable membership linkage; metadata-**minimized**, not eliminated.
2. **Malicious relay peer** — forwards/drops/replays/delays/modifies ciphertext. *Guarantee:* SFrame E2E (cannot decrypt) + frame auth + anti-replay window (active tampering detected & rejected).
3. **Malicious group member** — valid membership, modified client, can save/screenshot/export plaintext and keys. *Non-guarantee:* retention, shred, and tombstones are **cooperative controls** binding honest clients only; content a malicious recipient already decrypted **cannot** be made unrecoverable.

**Precise infrastructure claim** (replaces "no central server ever routes media"):
> No central server stores plaintext, content keys, durable membership state, or persistent content. Infrastructure may assist discovery/wake/NAT-traversal, and **TURN may relay encrypted packets it cannot decrypt**. "Serverless" = shorthand for this definition only.

**Metadata claim:** content-private by design; **metadata-minimizing, NOT metadata-anonymous** (timing/endpoint-churn traffic analysis is acknowledged, out of v1 scope).

**Infrastructure-metadata matrix** (release-gated by AC-METADATA):
| Component | Content? | IP? | Timing? | Persists linkage? |
|-----------|----------|-----|---------|-------------------|
| Signaling/rendezvous | No | Yes | Yes | No (ephemeral in-memory) |
| STUN | No | Yes | Yes | Provider-dependent |
| TURN | Ciphertext only | Yes | Yes | Provider-dependent |
| Push (FCM) | No content | Device token | Wake timing | Provider-dependent |
| Peer relay | SFrame ciphertext only | Peer endpoint | Timing/topology | Local only |
| Volunteer storage relay (optional) | Ciphertext only | Peer endpoint | Timing | Local only |

**Deletion wording (mandatory UX copy):** crypto-shred *destroys the keys your client controls and asks honest clients to purge*; it **cannot** remove screenshots, copied plaintext, backups, modified clients, or exported data. **Cross-device caveat (consensus):** an offline-but-honest *own* device retains keys until it reconnects and syncs the tombstone, so the honest claim is *"deleted on your online devices now; pending on offline devices until they reconnect"* — never "deleted everywhere." Native shred zeroizes enumerated key stores incl. SQLite/WAL; strong-but-not-absolute (OS swap/crash-dumps/FS-snapshots remain caveats). **Presence residual (consensus):** a malicious *member* (adversary 3) can still infer author liveness from whether a returned archival key decrypts; AC-PRESENCE closes only the *non-member* presence leak.

---

## 1. Requirements Summary

discrypt is an open-source, **content-private serverless** (per §0), E2E-encrypted Discord-style **text + voice** app on **Tauri 2** (Rust + **React** UI) for **Windows/macOS/Linux/Android** (Web on a later track; iOS → v2). Hard invariant per §0.

Distilled v1 obligations:

- **Identity + multi-device:** local keypair; **each device is its own MLS leaf**; device add via existing-device authorization (QR pairing); device-change transparency notices; device removal/rotation evicts the leaf + rekeys. Backup/restore, lost-passphrase, compromised-device rotation are v1. Out-of-band add via **friend-codes/QR with explicit MITM safety-number verification** (no directory).
- **Servers/DMs + admission:** join by **invite link** (room secret) with **expiry / revocation / max-use**; **optional rate-limited memory-hard password** (PAKE-style considered), verified independently of the room secret; **final admission via an authorized MLS add/commit or expiring Welcome** — the link alone is insufficient. Roles owner/admin/member.
- **Room governance:** roles/invites/bans/retention-policy are **signed, ordered, epoch-bound MLS events** (governance log) with defined mutation authority + concurrent/offline conflict resolution.
- **Group crypto:** **OpenMLS** (RFC 9420) + a **delivery/ordering/Welcome/catch-up/rejoin layer** with **fork/downgrade/replay detection**. Per-message/epoch content keys.
- **Text + history:** MLS-encrypted; **per-author logs** (multi-device merged); broadcast over the overlay on coming online; **opportunistic store-and-forward** (held as ciphertext by relays / optional **content-blind volunteer relays**) — delivery is **not guaranteed** without stable peers/relays.
- **Retention (locked, v1.2):** per-author cached window; **default 7 days**; **presets 1 h / 24 h / 7 d / 30 d / 90 d + custom + explicit warned "unlimited / never-lock" opt-in**; **lock-not-vanish**; **shorten = retroactive, lengthen = future-only**; >window served live by the author's **membership-proven, rate-limited** device(s) (author-as-KMS). Any of the author's online devices can serve.
- **Deletion (locked):** retention boundary = **lock, not delete**. Crypto-shred is a **cooperative** control (§0): destroys author-controlled keys + tombstones for delivered copies; account destruction wipes local + renders distributed ciphertext undecryptable on honest **online** nodes. **Cross-device:** shred propagates to all of the author's devices **best-effort** (an offline own device retains keys until it reconnects and checks tombstones before serving).
- **Voice (locked):** WebRTC; mesh ≤~8; beyond that the **adaptive self-healing peer-relay overlay** (≤3 hops, energy-aware ranking, failover + per-packet re-delivery). **SFrame** keyed from MLS exporter; relays cannot decrypt; active relays rejected. **Android voice is an explicit track** (webview Encoded-Transform is now the primary at-risk media path → webrtc-rs/native contingency).
- **Connectivity:** **STUN → relay-overlay → TURN (ciphertext only)**; signaling content-blind, zero linkage at rest; default + group-custom endpoints.
- **Wake:** content-free **FCM** (Android) — no content.
- **Abuse:** invite-flood/spam rate-limits; documented Sybil-resistance posture; relay-freeloading accounted for in ranking.
- **Non-goals (v1):** **Web platform** (later track), video/screen-share, granular roles, iOS, username directory, metadata anonymity / traffic-analysis resistance, binding a malicious recipient, true un-send, guaranteed availability past the author's window.

---

## 2. Architecture Overview

Single **Cargo workspace + a React UI** in a **Tauri 2 native** shell (Win/Mac/Linux/Android). Crates split by trust boundary and headless-testability. Overlay, signaling, media, MLS-delivery, and governance crates are UI-free for multi-node harness testing.

> **Web deferred (per ADR-D4):** the `transport` trait and WASM-friendly crate choices (RustCrypto/`aes-gcm`) are retained so a future Web track is cheap, but **no web shell, WASM bundle, DataChannel transport, or web-push code ships in v1**.

> **Media (per ADR-D1):** webview `RTCPeerConnection` carries transport + libwebrtc DSP; **all SFrame + MLS-exporter keys stay in Rust** via Insertable Streams (Encoded Transform) round-trips (JS never holds raw keys). **Android webview** is the at-risk env (Phase-1 gate); `media/transport.rs` (webrtc-rs) is the harness + **Android contingency**.

> **Multi-device (per ADR-D6):** one identity keypair; **each device = its own MLS leaf**. The user's own devices form a "device set"; per-author logs and content keys sync across the set; shred propagates across the set (best-effort until offline devices reconnect). **Honesty constraint (consensus):** an offline-but-honest own device may still hold live keys for a message the author already shredded, so the guarantee is scoped *"deleted on your online devices now; pending on offline until they reconnect"* — and every device MUST **check tombstones before serving any archival key on reconnect**, so a stale device cannot serve a shredded key.
>
> **Deterministic ordering (per ADR-D5 — consensus blocker resolved):** the gossip overlay (D2) yields only a *partial* order, but MLS commits, governance events, and device-leaf evictions all need a *total* order within an epoch. discrypt uses ONE **canonical comparator** everywhere: **(epoch number) → (committer/author MLS leaf index) → (signed content hash)**. `mls-delivery/ordering.rs`, `mls-core/governance.rs`, and `mls-core/device_set.rs` resolve every same-epoch race with this single rule. **Fork recovery:** on detected divergence the **comparator-maximal** history wins; the losing fork's commits are **re-applied as external-commits/proposals** against the winner (losing committers rejoin and replay), converging both honest partitions to one history within a bounded number of epochs.

```
discrypt/
├─ Cargo.toml / rust-toolchain.toml / deny.toml      # workspace + supply-chain gates
├─ crates/
│  ├─ mls-core/                      # OpenMLS wrapper + multi-device + governance
│  │  ├─ group.rs                    # create/join/add/remove → commits, log(N) rekey
│  │  ├─ identity.rs                 # identity keypair; PER-DEVICE leaves; safety-number; QR fingerprint
│  │  ├─ device_set.rs               # multi-device: pairing authz, device add/remove/rotate, transparency notices
│  │  ├─ exporter.rs                 # MLS exporter → SFrame media key + per-epoch content keys
│  │  ├─ governance.rs               # signed, ordered, epoch-bound role/invite/ban/retention events
│  │  └─ provider.rs                 # OpenMLS crypto/storage provider (RustCrypto)
│  ├─ mls-delivery/                  # NEW: the "missing service" around OpenMLS
│  │  ├─ ordering.rs                 # ordered commit delivery; epoch reconciliation; stale-proposal handling
│  │  ├─ welcome.rs                  # Welcome delivery; external-commit / rejoin / catch-up
│  │  └─ fork_detect.rs              # fork / downgrade / replay detection; never accept divergent history silently
│  ├─ content-keys/                  # crypto-shred + per-author retention lifecycle
│  │  ├─ keyring.rs                  # per-message/epoch content keys; cached-key (≤window) store
│  │  ├─ retention.rs                # per-author window: presets/custom/unlimited; shorten-retro / lengthen-future
│  │  ├─ policy.rs                   # author window declaration + propagation in epoch/message metadata
│  │  ├─ live_key.rs                 # author-as-KMS: membership-proof-gated + rate-limited + decoy responses
│  │  └─ shred.rs                    # cooperative shred; tombstones; CROSS-DEVICE propagation
│  ├─ storage/                       # local-only persistence
│  │  ├─ author_log.rs               # append-only per-author log (multi-device merge)
│  │  ├─ keystore.rs                 # OS-keychain-wrapped at-rest keystore (native)
│  │  ├─ device_sync.rs              # sync own logs/keys across the device set
│  │  ├─ secure_delete.rs            # zeroize + overwrite; enumerate SQLite/WAL + all key stores
│  │  ├─ backup.rs                   # backup/restore; lost-passphrase recovery
│  │  └─ recovery.rs                 # keystore snapshot/restore for two-phase shred
│  ├─ admission/                     # NEW: invite + password admission
│  │  ├─ invite.rs                   # invite encode/decode; expiry / revocation / max-use
│  │  └─ password.rs                 # rate-limited memory-hard password verify (PAKE-style considered)
│  ├─ transport/                     # transport abstraction (web impl deferred)
│  │  ├─ lib.rs                      # `Transport` trait: datagram + reliable-stream
│  │  └─ quic.rs                     # native impl over `quinn` (QUIC/UDP)
│  ├─ signaling/                     # content-blind rendezvous client + reference server (zero linkage at rest)
│  ├─ relay-overlay/                 # adaptive ALM (transport-agnostic)
│  │  ├─ topology.rs / ranking.rs    # capacity≈8, ≤3 hops; ping/stability/proximity/energy; anti-freeload
│  │  ├─ failover.rs / redelivery.rs # live re-route + per-packet seq/retransmit
│  │  ├─ gossip.rs                   # latency-tolerant text/history sync
│  │  ├─ integrity.rs                # SFrame frame auth + anti-replay window
│  │  └─ store_forward.rs            # opportunistic ciphertext queue (TTL); membership-gated; optional volunteer relays
│  ├─ media/                         # transport + SFrame E2E
│  │  ├─ transport.rs                # webrtc-rs path (harness + ANDROID contingency); ICE/STUN/TURN
│  │  ├─ transform_bridge.rs         # Insertable-Streams ↔ Rust SFrame bridge
│  │  ├─ sframe.rs                   # SFrame encrypt/decrypt (aes-gcm/RustCrypto); relays see ciphertext only
│  │  └─ jitter.rs / audio.rs        # harness DSP path (cpal + Opus)
│  ├─ abuse/                         # NEW: rate-limits, invite-flood/spam, Sybil posture, freeload accounting
│  ├─ push/                          # content-free FCM Android wake (opaque, no content)
│  └─ core/                          # domain orchestration for the Tauri shell
│     ├─ rooms.rs                    # Server/DM lifecycle; ties invite+governance+MLS admission
│     ├─ roles.rs                    # owner/admin/member enforcement via governance log
│     └─ commands.rs                 # #[tauri::command] surface
├─ apps/
│  ├─ ui/                            # React frontend (Discord-style UX)
│  │  ├─ src/                        # friends, servers, channels, voice room, invite flow, DEVICE mgmt, retention settings, verify UX
│  │  └─ src/media/transform.ts      # Insertable Streams wiring → Rust SFrame (no raw keys in JS)
│  └─ desktop/                       # Tauri shell (Win/Mac/Linux/Android targets)
│     └─ src-tauri/ (main.rs, tauri.conf.json)
├─ harness/multinode/                # headless N-node integration driver (no UI)
└─ .omc/ (specs, plans)
```

**Key dependency choices:** `openmls` + `openmls_rust_crypto`; webview `RTCPeerConnection` + Insertable Streams (prod) with `webrtc` (webrtc-rs) for harness + **Android contingency**; SFrame via `sframe-rs` or thin RFC-9605 over `aes-gcm`; `quinn` (QUIC) behind the `transport` trait; `argon2` (memory-hard password / passphrase KEK); `opus`+`cpal` (harness); `serde`/`rmp-serde`; `zeroize`; `tokio`. Pinned/vendored crypto deps. UI = **React**. (WASM-friendly choices retained for the future Web track; no web code in v1.)

---

## 3. Acceptance Criteria (concrete & testable)

- **AC1 — Identity/DM + verify:** Two fresh installs generate keypairs; A scans B's QR/friend-code; an E2E DM opens with **explicit MITM safety-number verification**; message round-trips, decryptable only by A & B; no directory/account server contacted (pcap: zero identity-content egress).
- **AC2 — Multi-device (NEW):** A user adds a 2nd device via existing-device authorization; the new device joins as its **own MLS leaf**, syncs history/keys, and other members receive a **device-added transparency notice**. Removing/rotating a device **evicts the leaf + rekeys**; the removed device loses access at the next epoch.
- **AC3 — Invite admission (REVISED):** Invite links **expire, are revocable, and honor max-use**; **final admission requires an authorized MLS add/commit or expiring Welcome** (link alone insufficient); a **rate-limited memory-hard password** gate is verified independently of the room secret.
- **AC4 — MLS text + per-author store:** Server text is MLS-encrypted; each user persists only its own sent messages (multi-device merged); broadcasts its author-log over the overlay on coming online.
- **AC5 — History sync at scale + ordering:** A 12–16 member server converges all author-logs via the gossip overlay with **ordered commit delivery + epoch reconciliation**.
- **AC6 — Group voice + overlay:** >8 participants → overlay carries media (≥1 hop, trees ≤3 deep), <150 ms direct + ≤+40 ms/hop.
- **AC7 — Failover recovery:** Killed relay re-routes within ≤3 s convergence; lost packets re-delivered (no permanent loss); audible gap ≤200 ms.
- **AC8 — Relays cannot decrypt (passive):** Relay given all bytes + DTLS material cannot recover plaintext (SFrame ciphertext only). Verified Phase-1 passthrough **and** re-verified end of Phase-2 over the real overlay.
- **AC8b — Active relay rejected:** Inject/drop/replay detected & rejected; anti-replay window rejects stale/duplicate counters.
- **AC-MLS-FORK (NEW):** Delayed/offline members **detect** divergence (epoch/tree-hash mismatch) and **never silently accept** a forked/downgraded/replayed history, **and recover deterministically** — after a fork, both honest partitions **converge to the comparator-maximal history within N≤2 epochs**, with the losing side's commits re-applied as external-commits/proposals. *Testable oracle:* the final agreed history equals the comparator-winner (not merely "no silent accept"). Asserted with an adversarial group-state-divergence node.
- **AC-GOV (NEW):** Role/invite/ban/retention-policy changes are **signed, ordered, epoch-bound** events; an unauthorized or out-of-epoch admin action is rejected; **concurrent offline admin changes resolve deterministically via the canonical comparator** (epoch → committer leaf index → signed content hash); a **removed admin cannot win a same-epoch race** (the eviction and the admin action are ordered by the same comparator; an action from a leaf evicted at-or-before the resolved epoch is rejected). *Testable oracle:* two conflicting offline admin events produce the **same resolved state on every honest client**.
- **AC9 — Opportunistic store-and-forward (REVISED):** A message to an offline recipient is held as ciphertext by relays / an optional **content-blind volunteer relay** and delivered+decrypted on return within the author's **current effective** window. *Interaction with retention (consensus):* if the author **shortens** their window (retroactive) below the queued message's age before the recipient returns, the delivered message **locks** (becomes a live-key placeholder, per lock-not-vanish) rather than decrypting from a cached key — the SF queue does **not** override a shortened window. SF holders chosen **without assuming always-on devices**; UX states delivery is **not guaranteed** without stable peers/relays.
- **AC10 — Per-author retention tier:** With the default 7-day window, ≤7-day messages decrypt offline; >7-day messages are **locked placeholders that do not vanish**, re-decrypting only while one of the author's devices is online.
- **AC10b — Retention config + semantics (REVISED):** Settings expose **presets (1 h/24 h/7 d/30 d/90 d) + custom + an explicit warned "unlimited / never-lock" opt-in**; **shortening re-locks existing messages sooner (retroactive); lengthening applies only to future messages**.
- **AC-PRESENCE (NEW, blocking):** Archival (live-key) requests are gated by **membership proof at the relevant epoch** (verified **locally** from a signed MLS group-state credential — no online lookup that would itself leak presence) + rate-limited; **a non-member cannot infer author online/offline status** (optional decoy responses). *Scope (consensus):* closes the **non-member** leak only — a malicious *member* can still infer author liveness from whether a returned key decrypts; that residual is acknowledged in §0 (adversary 3), not claimed solved.
- **AC11 — Crypto-shred (cooperative):** Author key destruction renders undelivered + archival messages unreadable on **honest online** clients/relays (incl. SF queues); tombstones purge delivered copies; **cross-device** shred propagates to the author's device set **best-effort** — an **offline own device retains keys until it reconnects and syncs the tombstone, and MUST check tombstones before serving any archival key**. UX copy: *"deleted on your online devices now; pending on offline until they reconnect"* — never "deleted everywhere."
- **AC-SHRED-PERSIST (NEW):** Negative tests prove no recoverable plaintext/keys in local **SQLite/WAL** or enumerated key stores after shred (native zeroization); two-phase shred (snapshot→verify→destroy) recoverable on failed verify.
- **AC12 — Account destruction:** Wipes all local data (all devices, best-effort) + renders the user's distributed ciphertext undecryptable on honest **online** nodes (an offline own device retains keys until it reconnects and syncs the destruction tombstone — same caveat as AC11/R15).
- **AC13 — Fallback chain:** STUN → relay-overlay → TURN (ciphertext only); owner overrides STUN/TURN endpoints; each leg activates under simulated NAT.
- **AC14 — Multi-platform build (REVISED):** Builds & runs natively on Win/macOS/Linux/Android from the Tauri toolchain. **Android voice path verified** (webview Encoded-Transform, else webrtc-rs/native contingency). (**Web is out of v1.**)
- **AC15 — Android wake:** Content-free FCM; pcap proves no message/identity content.
- **AC16 — Roles:** owner/admin/member enforced **via the governance log** (AC-GOV).
- **AC17 — Member-removal forward secrecy:** Removed/compromised member (or device) loses access at next epoch; SF queue refuses post-removal delivery.
- **AC18 — Signaling zero-metadata-at-rest:** pcap/inspection proves no persisted identity↔room↔topology linkage.
- **AC-METADATA (NEW):** Infrastructure metadata exposure matches the §0 matrix; pcap validates no central content egress, relay ciphertext-only, content-free push, signaling no-linkage.
- **AC-ABUSE (NEW):** Invite-flood/spam rate-limits enforced; Sybil-resistance posture documented; relay-freeloading reflected in ranking (freeloaders deprioritized).
- **AC-RECOVERY (NEW):** Backup/restore round-trips **identity keypair + device-set + room membership** for *account continuity* — explicitly **NOT a full archival content-key vault**. Lost-passphrase recovery restores **account access** (rejoin rooms as a member), **not** the ability to re-decrypt messages whose content keys were shredded or have locked. *Reconciliation with deletion-control (Principle 5, consensus):* because backups **exclude shreddable archival content keys**, a restore **cannot resurrect crypto-shredded content**; the only honest caveat is that a backup the user made *before* a shred is the user's own copy and outside shred's reach (stated plainly in UX). A compromised device is rotated out (leaf eviction + rekey) with identity preserved.

---

## 4. Implementation Steps (phased to de-risk hardest parts first)

> **Sequencing principle:** prove the two novel sink-risks — **SFrame-through-relay** and the **adaptive overlay** — in headless harnesses before UX. Then build the correctness-critical crypto-state machinery (MLS robustness, governance, multi-device, retention/shred). UX last.

**Phase 0 — Workspace + identity/multi-device spine + supply-chain.**
- Cargo workspace, `rust-toolchain.toml`, CI matrix (Win/Mac/Linux + Android NDK). Supply-chain gates (`cargo-audit`, `cargo-deny`/`deny.toml`, SBOM, reproducible posture, pinned/vendored crypto).
- Scaffold `transport` trait + native `quic.rs`.
- `mls-core/identity.rs` + `device_set.rs` + `storage/keystore.rs`: keypair, **per-device MLS leaves**, OS-keychain at-rest, friend-code/QR + **safety-number**, device pairing authz.
- `mls-core/group.rs` + `exporter.rs`: create/add/remove → commits; exporter secret.
- *Gate:* 16-member MLS group add/remove with log(N) rekey + stable exporter; **a single identity with 2 device-leaves** participates correctly; CI fails on CVE/license; SBOM emitted.
- *Rollback:* VCS revert; no runtime state.

**Phase 1 — SFrame E2E media slice + D1 bake-off (RISK #1).**
- D1 A/B/synthesis bake-off; `media/sframe.rs` keyed from exporter; `transform_bridge.rs`; passthrough relay node.
- *Gate (AC8):* relay cannot decrypt; AEC + jitter smoke tests; **encoded-frame hooks verified on Android webview specifically** (now the primary at-risk path since Web is out). 
- *Rollback:* if Android webview hooks fail, **Android voice falls back to the webrtc-rs/native path** (explicit track) before Phase 2; desktop ships the synthesis.

**Phase 2 — Adaptive relay overlay (RISK #2).**
- `topology.rs`/`ranking.rs` (capacity≈8, ≤3 hop, energy-aware, **anti-freeload**); `failover.rs`/`redelivery.rs`; `integrity.rs` (frame auth + anti-replay); carry Phase-1 media over the overlay.
- *Gates:* AC6, AC7 (≤3 s convergence, ≤200 ms gap, no permanent loss, thrash ≤1/30 s, hop ≤3); AC8 re-run over the real overlay; AC8b adversarial relay.
- *Rollback:* shrink v1 voice max size; mesh-only ≤8 interim.

**Phase 3 — Text, per-author logs, MLS delivery layer, gossip, store-and-forward.**
- `storage/author_log.rs` (multi-device merge) + `device_sync.rs`; `relay-overlay/gossip.rs` + `store_forward.rs` (membership-gated, opportunistic, optional volunteer relays); **`mls-delivery/` ordering + welcome + fork_detect**.
- *Gates:* AC4, AC5, AC9; **AC-MLS-FORK** (adversarial divergence node — never silently accept forked history).
- *Rollback:* feature-flag SF off → online-only; if fork-recovery is incomplete, gate group features as beta.

**Phase 4 — Retention + crypto-shred + author-as-KMS + member-removal FS (RISK #3).**
- `content-keys/{keyring,retention,policy,live_key,shred}.rs`; `storage/{secure_delete,recovery,backup}.rs`.
- Retention: presets/custom/**warned-unlimited**; **shorten-retroactive / lengthen-future**; lock-not-vanish.
- `live_key.rs`: **membership-proof-at-epoch + rate-limit + decoy** (AC-PRESENCE — **blocking**, not a follow-up).
- `shred.rs`: cooperative shred + tombstones + **cross-device propagation**; two-phase (snapshot→verify→destroy).
- *Gates:* AC10, AC10b, AC11, AC-PRESENCE, AC-SHRED-PERSIST (SQLite/WAL negative tests), AC12, AC17.
- *Rollback:* freeze retention as "beta/unverified" in UX until negative shred tests pass; restore from `recovery.rs` snapshot on shred-verify failure.

**Phase 5 — Governance + admission + identity recovery + abuse.**
- `mls-core/governance.rs` (signed, ordered, epoch-bound role/invite/ban/retention events; mutation authority; concurrent/offline conflict resolution; removed-admin race handling); `admission/{invite,password}.rs` (expiry/revoke/max-use; rate-limited memory-hard password; MLS-commit/Welcome admission); `storage/backup.rs` recovery flows (backup/restore, lost-passphrase, **compromised-device rotation**); `abuse/` (invite-flood/spam rate-limits, Sybil posture, freeload accounting).
- *Gates:* AC-GOV, AC3, AC-RECOVERY, AC-ABUSE, AC16.
- *Rollback:* governance/admission are additive; revert to prior known-good event schema; recovery flows behind feature flags.

**Phase 6 — Connectivity completion + signaling + push.**
- `signaling/` client + reference server (**zero linkage at rest**); STUN→overlay→TURN fallback; owner overrides; `push/` content-free FCM.
- *Gates:* AC1–AC3, AC13, AC15, AC18, **AC-METADATA** (matrix validated by pcap).
- *Rollback:* configurable endpoints; revert reference server; group-custom fallback.

**Phase 7 — Tauri shell + Discord UX + multi-platform.**
- `core/` + `#[tauri::command]`; `apps/ui` React (friends, servers, channels, voice room, invite flow, **device management**, **retention settings** incl. warned-unlimited, **verification UX**, honest deletion/availability copy per §0); `apps/desktop` (Win/Mac/Linux/Android).
- *Gate:* AC14, AC16; native builds on all four targets; Android voice path verified; end-to-end demo.
- *Rollback:* revert UI build; backend crates intact.

### Per-phase Rollback summary
| Phase | Failure | Recovery |
|-------|---------|----------|
| 0 | Bad scaffold / multi-device leaf bug / CVE gate | VCS revert; fix `deny.toml`; isolate device_set |
| 1 | Android webview Encoded-Transform unusable | Android → webrtc-rs/native track; desktop keeps synthesis |
| 2 | Convergence/thrash unmet | Shrink voice max; mesh-only ≤8 interim |
| 3 | History/SF/fork regression | Flag SF off; gate group features beta until fork-recovery passes |
| 4 | Shred corrupts keystore | Two-phase shred + `recovery.rs` snapshot restore |
| 5 | Governance/admission/recovery regression | Revert event schema; flag recovery flows off |
| 6 | Signaling regression / metadata leak | Configurable endpoints; revert reference server |
| 7 | UI regression | Revert UI build; backend intact |

---

## 5. Risks and Mitigations

| # | Risk | Mitigation |
|---|------|------------|
| R1 | No production-grade Rust SFrame crate. | Evaluate `sframe-rs`; else thin RFC-9605 over `aes-gcm` with test vectors + independent review; pinned/vendored. |
| R2 | webrtc-rs gaps / **Android** webview encoded-frame parity (now the primary at-risk path). | D1 synthesis + Phase-1 Android gate; **webrtc-rs/native is the explicit Android contingency track**, not just a note. |
| R3 | Overlay thrash/instability. | Hysteresis/damping; thrash ≤1/30 s; convergence ≤3 s; hop ≤3; soak tests. |
| R4 | Failover loss audible. | `redelivery.rs` → no permanent loss; concealment; gap ≤200 ms (AC7). |
| R5 | Author-as-KMS "locked" UX looks like data loss. | Clear "locked, author offline" copy; 7-day default hits this sooner; covered by AC10. |
| R6 | Crypto-shred incomplete (caches/swap/WAL). | `zeroize` + enumerate stores incl. SQLite/WAL; two-phase shred; AC-SHRED-PERSIST negatives. **§0: strong-not-absolute; honest UX.** |
| R7 | Android background/socket kills. | Content-free FCM; foreground service for calls; energy-weighted ranking. |
| R8 | Signaling metadata honeypot / traffic analysis. | Zero linkage at rest; opaque blobs; self-hostable; AC18/AC-METADATA. **§0: metadata-minimizing, NOT anonymous.** |
| R9 | NAT traversal failures. | STUN→overlay→TURN (ciphertext only); per-leg AC13. |
| R10 | **MLS state divergence / fork / downgrade / replay.** | `mls-delivery/` ordering + reconciliation + `fork_detect.rs`; **AC-MLS-FORK** adversarial divergence tests; persist epoch state. |
| R11 | Scope creep / "full Discord clone." | Non-goals are hard gates; **Web is out of v1**; video/iOS/granular-roles rejected at review. |
| R12 | Malicious/active relay. | `integrity.rs` frame auth + anti-replay; AC8b. |
| R13 | Evicted member/device still served. | Membership-gated SF; next-epoch access loss; AC17. |
| R14 | **Author-as-KMS presence leak** (now blocking). | Membership-proof-at-epoch + rate-limit + decoy responses; AC-PRESENCE is a Phase-4 gate, not a follow-up. |
| **R15** | **Multi-device complexity** — cross-device shred consistency (offline device holds keys), cross-device log/key merge, pairing security. | Device set with existing-device-authorized pairing; shred is **propagated + best-effort** until offline devices sync (UX states this); deterministic log merge; AC2 + AC-RECOVERY. |
| **R16** | **Governance-log conflicts** — concurrent/offline admin changes, removed-admin race, out-of-epoch actions. | Signed epoch-bound events resolved by the **canonical comparator** (epoch → committer leaf index → signed content hash) shared with `mls-delivery` + `device_set`; removed-admin cannot win a same-epoch race; reject out-of-epoch/unauthorized; AC-GOV. |
| **R17** | **Store-and-forward availability gap** — no stable peers ⇒ no offline delivery. | Opportunistic by design; **optional content-blind volunteer/self-host relays**; honest "not guaranteed" UX; AC9. |
| **R18** | **Invite-link secret leakage** (history/clipboard/previews/sync). | Expiry/revocation/max-use; rate-limited memory-hard password; **final admission via MLS commit/Welcome**, not the link alone; AC3. |
| **R19** | **Malicious recipient** (cooperative-control limit). | Documented §0 non-guarantee; honest UX; not crypto-solvable — do not over-claim deletion. |
| **R20** | **MITM on friend-code/QR add** if users skip verification. | Explicit **safety-number verification UX**; device-change transparency notices; AC1/AC2. |
| **R21** | **Abuse** (spam/invite-flood/Sybil/relay-freeload). | `abuse/` rate-limits; documented Sybil posture; freeload-aware ranking; AC-ABUSE. |
| **R22** | **Backup/recovery defeats crypto-shred** — a recoverable backup could resurrect shredded content. | Backups are **account-continuity only** (identity + device-set + room membership); they **exclude shreddable archival content keys**, so a restore cannot resurrect shredded content; honest UX that a user's own pre-shred backup is outside shred's reach. AC-RECOVERY. |

---

## 6. Verification Steps

- **Per-phase gates** are blocking.
- **Crypto review pass:** `mls-core`, `mls-delivery`, `content-keys`, `media/sframe.rs`, `relay-overlay/integrity.rs`, `admission/`, `governance.rs` get an independent review lane (`document-specialist` vs RFC 9420/9605 + OpenMLS; `code-reviewer` for impl) — never self-approved.
- **Network capture:** AC1, AC8, AC15, AC18, **AC-METADATA** (full matrix).
- **Supply-chain CI:** `cargo-audit` + `cargo-deny`; SBOM; `Cargo.lock`; pinned/vendored crypto.
- **Multi-node soak:** 16-node sessions with churn/latency/loss; p50/p95 mouth-to-ear vs budget; re-parent vs ≤1/30 s; convergence vs ≤3 s.
- **Adversarial harness:** malicious relay (AC8b), evicted member/device (AC17), **group-state-divergence node (AC-MLS-FORK)**, out-of-epoch/unauthorized admin (AC-GOV).
- **Crypto-state tests:** multi-device leaf add/remove + cross-device shred; retention shorten-retro / lengthen-future; SQLite/WAL shred negatives (AC-SHRED-PERSIST); backup/restore + device rotation (AC-RECOVERY).
- **Platform CI:** Win/macOS/Linux + Android emulator (incl. Android voice path) before any release tag. **Dual media stack (consensus):** if the Android webrtc-rs/native contingency is active, AC6/AC7/AC8 are a **recurring matrix run on BOTH stacks** (desktop webview+libwebrtc *and* Android webrtc-rs), not a one-time Phase-1 switch — the SFrame/AEC/jitter integration is maintained on both for the life of v1.
- **Negative tests:** decryption fails post-shred and from relay vantage; locked-not-vanished messages still exist as ciphertext; non-member cannot infer presence (AC-PRESENCE).

---

## 7. RALPLAN-DR Summary

**Mode:** DELIBERATE.

### Principles
1. **Precise guarantees over absolutes (§0)** — claim only what holds; "serverless" = no plaintext/keys/durable-membership/persistent-content on infra; TURN relays ciphertext; metadata-minimizing not anonymous; cooperative controls bind honest clients only.
2. **De-risk physics before polish** — SFrame-through-relay + overlay first; crypto-state machinery (MLS robustness, governance, multi-device, retention/shred) second; UX last.
3. **Crypto is locked; build glue, not constructions** — OpenMLS/SFrame + a delivery/fork-detection layer; independent review lane; pinned/vendored.
4. **Test by adversary** — relay can't decrypt; active relay rejected; forked history rejected; out-of-epoch admin rejected; non-member can't infer presence; shred irreversible on honest nodes.
5. **Deletion-control > availability; lock ≠ delete** — retention boundary locks; only explicit shred destroys (cooperatively).

### Top Decision Drivers
1. Relays must never decrypt (SFrame, passive + active).
2. Voice latency <150 ms direct / ≤+40 ms hop with self-healing overlay.
3. Serverless (per §0) + author-as-KMS retention/shred (per-author, presets+warned-unlimited, shorten-retro/lengthen-future) + member/device-removal forward secrecy.
4. **Correct distributed crypto-state** — MLS fork/downgrade/replay recovery + signed governance + multi-device leaves are first-class, not afterthoughts.

### Resolved Architecture Decisions (see ADR)
- **D1 (media):** synthesis (webview transport + libwebrtc DSP, Rust-only SFrame/keys). **Android webview at-risk → webrtc-rs/native contingency track.**
- **D2 (overlay):** decentralized gossip-scored self-org + hysteresis + energy/freeload-aware ranking; bootstrap-only signaling hint.
- **D3 (signaling):** minimal self-hostable QUIC reference, opaque blobs, zero linkage at rest.
- **D4 (web/transport — REVISED):** **Web removed from v1** (later track). `transport` trait + WASM-friendly crate choices retained; native = `quinn`/QUIC; web/DataChannel impl deferred.
- **D5 (governance — NEW):** room state (roles/invites/bans/retention) = signed, ordered, epoch-bound MLS events.
- **D6 (multi-device — NEW):** per-device MLS leaves under one identity; existing-device-authorized pairing; cross-device sync; cross-device shred (best-effort until sync); transparency notices.
- **D7 (admission — NEW):** expiring/revocable/max-use invites + rate-limited memory-hard password (PAKE-style considered) + MLS-commit/Welcome final admission.
- **D8 (retention bounds — NEW):** presets + custom + warned-unlimited opt-in; shorten=retroactive, lengthen=future-only; default 7 d.

---

## 8. Pre-mortem (deliberate mode) — failure scenarios that sink v1

1. **SFrame-through-relay never achieves E2E** — esp. **Android webview** encoded-frame hooks unusable. *Trip-wire:* not green by end of Phase 1 → Android falls to webrtc-rs/native; desktop ships synthesis.
2. **Overlay thrashes / blows latency budget.** *Trip-wire:* enforce hop≤3 + hysteresis; else shrink voice max / mesh-only ≤8 interim.
3. **Crypto-shred not irreversible** (keys survive in caches/swap/WAL). *Trip-wire:* freeze retention as beta until SQLite/WAL negatives pass; §0 honest UX; two-phase shred.
4. **MLS group-state forks silently** — offline/delayed members accept divergent/downgraded/replayed history. *Sinks correctness.* *Early-warning:* AC-MLS-FORK fails. *Trip-wire:* gate all group features as beta until `fork_detect.rs` + reconciliation pass.
5. **Multi-device shred/sync inconsistency** — an offline device retains keys for a "shredded" message; cross-device log merge diverges. *Early-warning:* AC2/AC11 cross-device tests fail. *Trip-wire:* ship shred as **propagated + best-effort with explicit UX**; block "deleted everywhere" copy; if merge is unreliable, restrict to single active device per session until fixed.
6. **Over-claimed deletion/serverless/anonymity** — UX promises more than §0 delivers. *Sinks trust/legal.* *Trip-wire:* all release copy reviewed against §0; AC-METADATA + AC11 wording gates.
7. **Backup/recovery silently defeats deletion-control** — lost-passphrase recovery or a restore resurrects content the author shredded, contradicting Principle 5. *Early-warning:* an AC-RECOVERY restore re-decrypts a shredded/locked message. *Trip-wire:* backups are account-continuity only (no archival content-key vault); a restore that re-decrypts shredded content fails the gate; UX states user-made pre-shred copies are outside shred's reach.

---

## 9. Expanded Test Plan (deliberate mode)

**Unit**
- `mls-core`: group add/remove, log(N) rekey, exporter determinism, next-epoch access loss; **per-device leaf** add/remove; `governance.rs` signed-event authority/ordering.
- `mls-delivery`: ordered delivery; epoch reconciliation; stale-proposal; Welcome/rejoin/catch-up; **fork/downgrade/replay detection**.
- `content-keys`: retention presets/custom/unlimited; **shorten-retro / lengthen-future**; lock-not-vanish; `live_key` membership-proof + rate-limit + decoy; cross-device shred; two-phase snapshot/restore.
- `media/sframe`: RFC-9605 vectors; round-trip; tamper reject; anti-replay.
- `relay-overlay`: ranking incl. energy + freeload; tree ≤3; redelivery seq-window; integrity/anti-replay.
- `admission`: invite expiry/revoke/max-use; rate-limited memory-hard password.
- `transport`: trait conformance (quic).

**Integration**
- MLS exporter → SFrame key match at both ends.
- Insertable-Streams ↔ Rust SFrame: no raw keys in JS.
- Author-log persistence + gossip merge converges (incl. **two devices of one user**).
- STUN→overlay→TURN fallback under simulated NAT (AC13).
- Governance event applied across members; out-of-epoch/unauthorized rejected (AC-GOV).

**E2E (`harness/multinode`)**
- Relays-can't-decrypt passive (AC8) — Phase-1 + Phase-2 over real overlay. *Highest priority.*
- Active relay inject/drop/replay rejected (AC8b).
- **Group-state divergence (AC-MLS-FORK):** adversarial node forks/replays; assert **detect** (reject divergent history) **and recover** — both honest partitions converge to the **comparator-maximal** history within N≤2 epochs (losing side re-applies as external-commits). *Oracle:* final agreed history equals the comparator-winner, not merely "no silent accept."
- Failover recovery (AC7): ≤3 s convergence, ≤200 ms gap, no permanent loss.
- Member/device-removal FS (AC17): evict → next-epoch access loss; SF refuses post-removal delivery.
- **Multi-device (AC2):** add/remove/rotate device; cross-device sync + transparency notice.
- Crypto-shred (AC11/AC-SHRED-PERSIST/AC12): cross-device shred; no recoverable plaintext/keys incl. SQLite/WAL; tombstone purge.
- Retention transition (AC10/AC10b): default-7d boundary; author-set 24 h shortens retroactively; lengthen applies future-only.
- **Presence (AC-PRESENCE):** non-member archival-key request cannot infer online/offline.
- History sync at scale (AC5) + opportunistic SF (AC9, incl. optional volunteer relay).
- Signaling zero-metadata (AC18) + infra matrix (AC-METADATA).
- Recovery (AC-RECOVERY): backup/restore (account-continuity only) + device rotation; **negative test: a restore CANNOT re-decrypt a previously shredded/locked message** (deletion-control vs recovery).
- Governance conflict (AC-GOV): two conflicting offline admin events + a removed-admin same-epoch action → assert identical resolved state on every honest client via the canonical comparator.

**Observability:** p50/p95 mouth-to-ear vs budget; hop depth; re-parent vs ≤1/30 s; convergence vs ≤3 s; loss/redelivery; topology snapshots (content-free); shred audit log + two-phase markers; integrity/anti-replay counters; governance-event audit; pcap matrix assertions. All telemetry local/opt-in, content-free.

---

## 10. Architecture Decision Record (ADR)

### Decision
- **D1 (media):** Synthesis — webview `RTCPeerConnection` + libwebrtc DSP; **SFrame + MLS-exporter keys in Rust** via Insertable Streams (JS never holds raw keys). `media/transport.rs` (webrtc-rs) = harness + **Android contingency** (Android webview is the at-risk encoded-transform env now that Web is out).
- **D2 (overlay):** Decentralized gossip-scored self-org + hysteresis + energy/freeload-aware ranking; bootstrap-only signaling hint.
- **D3 (signaling):** Minimal self-hostable QUIC reference; opaque blobs; zero linkage at rest.
- **D4 (web/transport):** **Web removed from v1.** `transport` trait + WASM-friendly crate choices retained for a future Web track; native = `quinn`/QUIC; web/DataChannel + WASM + web-push deferred.
- **D5 (governance + canonical ordering):** Room state = signed, ordered, epoch-bound MLS events. A single **canonical comparator** — **epoch → committer/author leaf index → signed content hash** — provides the total order used by `mls-delivery/ordering.rs`, `mls-core/governance.rs`, AND `mls-core/device_set.rs` eviction. **Fork recovery:** comparator-maximal history wins; losers re-apply via external-commit/proposals. This resolves the consensus blocker that MLS needs a total order the gossip overlay (D2) does not natively provide, and gives AC-MLS-FORK / AC-GOV a concrete testable oracle.
- **D6 (multi-device):** Per-device MLS leaves under one identity; existing-device-authorized QR pairing; cross-device log/key sync; **cross-device shred propagated best-effort until sync**; device-change transparency notices.
- **D7 (admission):** Expiring/revocable/max-use invites + rate-limited memory-hard password (PAKE-style considered) + MLS-commit/Welcome final admission (link alone insufficient).
- **D8 (retention):** Presets (1h/24h/7d/30d/90d) + custom + explicit warned "unlimited/never-lock"; default 7 d; **shorten=retroactive, lengthen=future-only**; lock-not-vanish.

### Drivers
Relays never decrypt; voice latency budget; serverless-per-§0 + author-as-KMS retention/shred; correct distributed crypto-state (fork recovery, governance, multi-device); precise guarantees over absolutes.

### Alternatives considered
- **Scope — full multi-platform v1 (overlay+Web+SF+Android):** rejected — Web is the biggest scope multiplier; cutting it preserves the overlay (the reason-to-exist) while making v1 approvable.
- **Scope — native-desktop-only v1 (reviewer's full staging):** rejected — would defer the overlay (user's core conviction) out of v1; instead Web-only is cut and the overlay/Android/SF stay.
- **Retention — no ceiling:** rejected — silently undermines deletion-control; replaced by presets + warned-unlimited opt-in (longer-than-7d still allowed per user).
- **Multi-device — defer to v1.5:** rejected by user — promoted into v1 (closes device-migration/rotation/backup gaps at the cost of cross-device shred/sync complexity).
- **Multi-device phase-trim (considered, not taken):** ship device pairing/removal/transparency in v1 but defer *cross-device archival-key serving* to v1.5 — would remove the hardest cross-device shred-consistency surface. Not taken (user chose full multi-device in v1); retained as the documented **risk-reduction fallback** if Phase-4 cross-device shred consistency proves intractable.
- **D1-A (full webrtc-rs) / D1-B (JS SFrame):** A = infeasible DSP for desktop; B = raw keys in JS unacceptable; synthesis chosen, A retained as Android contingency.

### Consequences
- New crates: `mls-delivery` (ordering/welcome/fork-detect), `admission`, `abuse`; `mls-core` gains `device_set.rs` + `governance.rs`; `storage` gains `device_sync.rs` + `backup.rs`; `content-keys/live_key.rs` gains presence mitigation.
- Web surface (WASM/DataChannel/web-push/web-keystore) **removed from v1**; `transport` trait + RustCrypto choices kept to make the future Web track cheap.
- Multi-device makes **cross-device shred consistency** and **log merge** first-class risks (R15) and improves author-as-KMS availability (any device online can serve).
- All product copy is now gated against §0 (deletion/serverless/anonymity wording).
- AC-PRESENCE, AC-MLS-FORK, AC-GOV, AC-METADATA, AC-RECOVERY become release gates.
- A single **canonical comparator** (epoch → committer leaf index → signed content hash) is now load-bearing across `mls-delivery`, `governance`, and `device_set`; it is the shared testable oracle for AC-MLS-FORK and AC-GOV. Membership proof for author-as-KMS is verified **locally** from a signed group-state credential (no online lookup), so AC-PRESENCE does not re-introduce a presence signal.
- **Backups are account-continuity only** (identity + device-set + room membership), never an archival content-key vault — so recovery cannot resurrect shredded content (reconciles AC-RECOVERY with Principle 5).

### Follow-ups
- `sframe-rs` audit vs in-house thin layer (Phase 1).
- PAKE selection for password admission (Phase 5) — or keep rate-limited memory-hard verify if PAKE complexity isn't justified.
- Define the future **Web track** entry criteria (reduced-trust posture) when it's picked up.
- Decide volunteer-relay discovery/UX detail (optional, off by default).

---

## 11. Changelog

- **2026-05-24 — v1.3 (pending approval).** Consensus re-validation (deliberate): **Architect SOUND-WITH-CHANGES**, **Critic APPROVE-WITH-MERGE-LIST**. All merge items applied:
  - **(Blocking)** Single **canonical total-order comparator** (epoch → committer leaf index → signed content hash) shared by `mls-delivery/ordering.rs`, `governance.rs`, and `device_set.rs` (§2, D5, R16). **AC-MLS-FORK** gains a **recovery oracle** (comparator-max wins; losers re-apply via external-commit; converge ≤2 epochs). **AC-GOV** gains the comparator + a removed-admin same-epoch-race rule.
  - **(High)** Offline-own-device deletion weakening foregrounded in §0 + AC11 (+ tombstone-check-before-serve on reconnect); **AC-RECOVERY** backup model defined as **account-continuity-only** (excludes shreddable archival content keys) and reconciled with Principle 5 — a restore cannot resurrect shredded content.
  - **(Medium)** AC-PRESENCE scoped to **non-members** (member-liveness residual noted in §0; membership proof verified locally so it adds no presence signal); **AC12** scoped to honest **online** nodes; **AC9** shorten-retro-vs-queued-SF defined (delivered message locks, SF does not override a shortened window); Android **dual media stack** made a recurring AC6/7/8 matrix (§6).
  - **(Risk/test)** R16 names the comparator; new **R22** (backup vs shred); new **pre-mortem scenario 7** (recovery vs deletion-control); §9 adds a fork-recovery convergence oracle, a governance-conflict determinism test, and a recovery-cannot-resurrect-shred negative test.
  - **(ADR)** D5 augmented with the comparator + fork-recovery; multi-device **phase-trim** added as a documented risk-reduction fallback alternative.
- **2026-05-24 — v1.2 (superseded by v1.3).** Incorporated a third-party architecture/risk review (graded v1.1 "SOUND DIRECTION, NOT APPROVABLE AS v1 SCOPE") via a 3-round re-interview. Changes:
  - **Scope:** **Web removed from v1** (later track); overlay/MLS/voice/SFrame/store-and-forward/Android retained. Dropped web shell/WASM/DataChannel/web-push/web-keystore + AC19–21 + R15–R19(web) + web pre-mortem; kept `transport` trait + React.
  - **§0 Threat Model added:** 3 adversary classes; precise serverless/TURN claim; metadata matrix; metadata-minimizing-not-anonymous; cooperative-control statement; honest shred/deletion wording.
  - **Retention:** presets + custom + warned-unlimited opt-in; **shorten=retroactive / lengthen=future-only**; AC10/10b revised.
  - **Multi-device IN v1 (D6):** per-device MLS leaves; `device_set.rs` + `device_sync.rs`; cross-device shred (best-effort); AC2 + AC-RECOVERY; R15.
  - **Distributed crypto-state:** governance log (D5, `governance.rs`, AC-GOV); `mls-delivery` ordering/welcome/fork-detect (AC-MLS-FORK); R10/R16 expanded.
  - **Availability/identity/admission:** opportunistic SF + optional content-blind volunteer relays (AC9/R17); author-as-KMS presence-leak made **blocking** (AC-PRESENCE/R14); invite expiry/revoke/max-use + memory-hard password + MLS-commit/Welcome admission (D7, AC3, R18); MITM safety-number verify (AC1, R20); backup/restore + device rotation (AC-RECOVERY); abuse mitigations (`abuse/`, AC-ABUSE, R21).
  - **Android:** explicit voice track (webview Encoded-Transform at-risk → webrtc-rs/native contingency).
  - **Phasing:** re-sequenced into Phases 0–7; rollback table updated; SQLite/WAL shred negatives (AC-SHRED-PERSIST).
- **2026-05-24 — v1.1 (superseded by v1.2 on Web/UI).** Added Web + React + WASM + retention 30d→7d. v1.2 removes Web from v1, keeps React + 7d default.
- **2026-05-24 — v1.0.** Original consensus plan (Architect SOUND-WITH-CHANGES; Critic APPROVE-with-merge-list); merged A1–A4 + Critic-1–6 + live-key threat note; filled ADR D1–D3.
