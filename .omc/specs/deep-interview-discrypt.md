# Deep Interview Spec: discrypt — Decentralized E2E Voice & Text (Discord-style)

## Metadata
- Interview ID: discrypt-2026-05-24 (re-interview: discrypt-review-2026-05-24)
- Rounds: 12 (original) + 3 (review-incorporation)
- Final Ambiguity Score: 6% (review-incorporation pass; original 17.8%)
- Type: greenfield (original) / brownfield amendment (review-incorporation)
- Generated: 2026-05-24
- Revised: 2026-05-28 (v1.4 — incorporates plan-review fixes)
- Threshold: 0.2
- Threshold Source: default
- Status: PASSED

## Amendments (post-interview)
Forward-looking sections below reflect these amendments; the original 12-round Interview Transcript is preserved verbatim as a historical record.

- **v1.4 (2026-05-28) — Plan-review fixes supersede v1.3 where they conflict.** Closed re-review blockers: (a) the canonical comparator still orders same-epoch application events, but **does not imply replayability of divergent MLS commits**; fork handling is explicit re-add/reboot/external-commit repair, and only still-valid application-level governance/text events may be re-proposed and revalidated against the repaired winner epoch; (b) SFrame requires app-level media sender authentication through per-sender/per-device keys or MLS-signed `KID → MLS leaf/device` state, because SFrame alone is not per-sender auth; (c) password admission rate-limiting requires OPAQUE/PAKE or an online authorized admission helper, not an offline-copyable verifier; (d) own author-log is authoritative, while recipients may cache bounded received ciphertext and eligible keys within retention policy; (e) recovery requires existing authorized device, recovery code, or sealed account-continuity backup, and no-material lost-passphrase recovery is non-recoverable; (f) AC-PRESENCE is scoped to the non-member live-key authorization/decryptability oracle beyond generic transport reachability, not generic online/offline inference.
- **v1.3 (2026-05-24) — Superseded by v1.4 on fork repair, SFrame sender auth, password admission, recipient caching, recovery, and presence wording.** Consensus re-validation (Architect + Critic). Closed the blockers the consensus pass found: (a) a single **canonical total-order comparator** — **epoch → committer/author MLS leaf index → signed content hash** — shared by the MLS delivery layer, the governance log, and device-leaf eviction (the gossip overlay only gives a partial order); (b) fork recovery received a first oracle, later narrowed by v1.4 to repair/rejoin/re-proposal rather than replaying invalid divergent MLS commits; (c) **cross-device shred is honestly scoped** — an offline-but-honest own device retains keys until reconnect, must check tombstones before serving, and UX says "deleted on online devices now; pending on offline"; (d) **backup/recovery is account-continuity-only** (identity + device-set + room membership; NOT an archival content-key vault), so a restore cannot resurrect shredded content (reconciles with deletion-control); (e) **AC-PRESENCE scoped to non-members** (a malicious member can still infer liveness; membership proof verified locally, adding no presence signal); (f) account-destruction and shred scoped to honest **online** nodes; (g) shorten-retroactive retention **locks** queued store-and-forward messages rather than overriding the shortened window. See the plan §11 for the full merge list.
- **v1.2 (2026-05-24) — Third-party review incorporated.** A reviewer graded v1.1 "SOUND ARCHITECTURE DIRECTION, NOT APPROVABLE AS CURRENT v1 SCOPE." Resolved via a 3-round re-interview:
  - **Scope:** **Web REMOVED from v1** (deferred to a later track). v1 = **native desktop + Android** only, retaining the adaptive overlay, OpenMLS text, group voice, SFrame, store-and-forward, per-author retention, crypto-shred, author-as-KMS. (Cuts the biggest scope multiplier while keeping the user's stated reason-to-exist, the overlay.)
  - **Web trust:** moot for v1 (Web is out). The `transport` trait is retained as architecture so a future web track is cheap.
  - **Retention bounds:** presets (1 h / 24 h / 7 d / 30 d / 90 d) + custom + an **explicit, warned "unlimited / never-lock" opt-in**; default 7 d. No silent unbounded default.
  - **Retention policy-change semantics:** **shorten = retroactive** (you control your own keys), **lengthen = future-only** (cannot resurrect keys honest recipients already zeroized).
  - **Multi-device identity is now a v1 feature** (per-device MLS leaves, cross-device key/log sync, cross-device shred propagation, device add/remove, backup/restore).
  - **Claims precision, room-state governance, MLS robustness, availability, identity/recovery, admission hardening** adopted per the Threat Model and Constraints sections below.
- **v1.1 (2026-05-24) — superseded by v1.2 on the Web/UI points; retention default 30 d → 7 d retained.** Original v1.1 added Web + React + WASM; v1.2 removes Web from v1 but keeps **React** (Tauri UI) and the 7-day retention default.

## Threat Model & Security Guarantees (v1.2)
The reviewer's strongest recommendation: state precise guarantees, not absolutes.

**Adversary classes:**
1. **Passive infrastructure adversary** — observes signaling, STUN, TURN, push metadata, IPs, timing, topology during live sessions. *Mitigated:* sees no content/keys/durable-linkage; metadata-minimized, not eliminated.
2. **Malicious relay peer** — forwards/drops/replays/delays/modifies ciphertext. *Mitigated:* SFrame E2E (cannot decrypt) + application-defined media sender binding + receiver anti-replay window (active tampering rejected).
3. **Malicious group member** — has valid membership, can run a modified client, can save plaintext/keys, screenshot, export. *NOT mitigable by crypto:* retention, shred, and tombstones are **cooperative controls** that bind honest clients; they cannot make content a malicious recipient already decrypted unrecoverable.

**Precise infrastructure claim (replaces "serverless / no central server ever routes media"):**
> No central server stores plaintext, content keys, durable membership state, or persistent content. Infrastructure (signaling/rendezvous, STUN, TURN, push) may assist discovery, wake, and NAT traversal, and **TURN may relay encrypted packets it cannot decrypt**. Steady-state content is preferentially peer-to-peer or peer-relayed. "Serverless" is used only as shorthand for this definition.

**Metadata claim:** content-private by design; **metadata-minimizing, not metadata-anonymous**. Timing/endpoint-churn traffic analysis is out of v1 scope (acknowledged, not claimed solved).

**Infrastructure-metadata matrix:**
| Component | Sees content? | Sees IP? | Sees timing? | Persists linkage? |
|-----------|---------------|----------|--------------|-------------------|
| Signaling/rendezvous | No | Yes | Yes | No (ephemeral in-memory only) |
| STUN | No | Yes | Yes | Provider-dependent |
| TURN | Encrypted packets only | Yes | Yes | Provider-dependent |
| Push (FCM) | No content | Device token | Wake timing | Provider-dependent |
| Peer relay | Encrypted (SFrame) only | Peer endpoint | Timing/topology | Local only |
| Volunteer storage relay (optional) | Ciphertext only | Peer endpoint | Timing | Local only (content-blind) |

**Deletion guarantee wording:** crypto-shred *destroys the keys your client controls and asks honest clients to purge recent copies*; it **cannot** remove screenshots, copied plaintext, backups, modified clients, or already-exported data. **Cross-device caveat:** an offline-but-honest *own* device retains keys until it reconnects and syncs the tombstone, so the honest claim is *"deleted on your online devices now; pending on offline until they reconnect"* — never "deleted everywhere." Native shred zeroizes controlled key material across enumerated stores (incl. SQLite/WAL); strong-but-not-absolute (OS swap, crash dumps, FS snapshots remain caveats). **Presence residual:** a malicious *member* can still infer author liveness from whether a returned archival key decrypts; the presence mitigation closes only the *non-member live-key success/failure oracle beyond generic transport reachability*, not metadata anonymity.

## Topology
All five original components remain active in v1 **except Web is removed from the platform surface**.

| Component | Status | Description | Coverage / Deferral Note |
|-----------|--------|-------------|--------------------------|
| 1. App Shell & UX | active | Tauri 2 (Rust + **React** UI) native app; Discord-style servers/channels/DMs, friends, invite flows, **multi-device** | v1: Windows/macOS/Linux/Android. **Web deferred** (later track). iOS, video, screen-share → v2. |
| 2. Signaling & Connectivity | active | STUN/TURN, signaling rendezvous, room negotiation, NAT traversal | STUN → relay-overlay → TURN; zero linkage at rest; **TURN relays ciphertext (cannot decrypt)**. |
| 3. Identity & Membership | active | Local keypair identity, **multi-device (per-device MLS leaves)**, friend-codes/QR + MITM verification, join-by-link, roles via **signed governance log** | v1 roles owner/admin/member as signed epoch-bound MLS events. No username directory. |
| 4. E2E Text & History Replication | active | OpenMLS text, per-author logs, overlay sync + ordering layer, opportunistic store-and-forward (+ optional volunteer relays), per-author retention (lock-not-vanish), crypto-shred | Core v1. MLS fork/replay recovery in scope. |
| 5. E2E Voice | active | WebRTC voice; mesh ≤~8 + adaptive self-healing peer-relay overlay; SFrame E2E through relays | v1-core. **Android voice is an explicit track** (webview Encoded-Transform at-risk → webrtc-rs/native contingency). |

## Goal
discrypt is an open-source, **content-private serverless** (per the Threat Model definition) end-to-end-encrypted Discord-style **voice + text** application built with **Tauri 2** (Rust backend + **React** UI) for **Windows, macOS, Linux, and Android** (Web on a later track; iOS in v2). It supports 1:1 DMs and multi-user "servers" with text channels and group voice, with **multi-device** identities. No central server stores plaintext, content keys, durable membership state, or persistent content. Connectivity is peer-to-peer with the fallback chain **STUN → adaptive peer-relay overlay → TURN (last resort, relays ciphertext only)**. Identity is a **local keypair** with **per-device MLS leaves**, exchanged via **friend-codes/QR with explicit MITM verification**; "servers" are joined by **invite link** (carrying the room secret, with expiry/revocation/max-use) gated by an optional password flow that uses **OPAQUE/PAKE or an online authorized admission helper** for real rate-limits, with final admission via an **authorized MLS add/commit or expiring Welcome**. Content is **E2E encrypted with OpenMLS** (RFC 9420); voice is protected through relays with **SFrame** (RFC 9605) keyed from the MLS exporter secret; room governance (roles/invites/bans/retention policy) is a **signed, ordered, epoch-bound governance log**. Message history uses a **per-author tiered retention model** (default 7-day cached window; presets + custom + warned unlimited opt-in; lock-not-vanish; author-as-KMS live-key archival served only to membership-proven requesters), with **crypto-shredding** as a cooperative deletion control.

## Constraints
- **Core invariant (precise):** no central server stores plaintext, content keys, durable membership state, or persistent content. See Threat Model for the full infrastructure claim.
- **Allowed infrastructure (content-blind):** STUN; TURN (relays ciphertext only; default-hosted + group-custom); signaling/rendezvous (room-name+password→peers, identity-key→endpoint; ephemeral, zero linkage at rest); content-free **FCM** Android wake; optional **content-blind volunteer/self-hosted storage relays** (ciphertext + TTL, no keys/plaintext) for groups that want better offline availability.
- **Media topology:** mesh ≤~8; beyond that the adaptive peer-relay overlay carries media (peers relay for ~8 others; ranked by ping/stability/proximity/energy; doze-prone mobiles de-prioritized as relays/SF holders; shallow ≤3-hop trees; live failover + per-packet re-delivery).
- **E2E through relays:** SFrame keyed from MLS exporter; relays handle ciphertext only; keys stay in Rust. Media sender identity is authenticated by per-sender/per-device SFrame keys or MLS-signed `KID → MLS leaf/device` state; SFrame alone is not treated as per-sender authentication. Active relays (inject/drop/replay) rejected via receiver anti-replay and tamper checks.
- **Crypto stack (locked):** OpenMLS (MIT, RFC 9420) + ephemeral per-message/epoch content keys + crypto-shred + SFrame. OpenMLS requires a surrounding **delivery/ordering/Welcome/catch-up/rejoin/repair layer** (built in v1) with **fork/downgrade/replay detection**; fork repair uses re-add/reboot/external-commit repair and re-proposes only still-valid application events, never invalid divergent MLS commits.
- **Identity (v1.4):** **multi-device** — each device is its own MLS leaf under one identity keypair; device add via existing-device authorization (QR pairing); recipients are notified of new devices (transparency); device removal/rotation evicts the leaf and rekeys. Backup/restore, existing-device recovery, recovery-code/sealed-backup recovery, and compromised-device rotation are v1; without an authorized device/recovery code/sealed backup, lost-passphrase/account recovery is non-recoverable.
- **Room governance:** roles (owner/admin/member), invites, bans, and retention policy are **signed, ordered, epoch-bound MLS events** (governance log) with defined mutation authority and concurrent/offline conflict resolution.
- **Admission:** invite links **expire, are revocable, support max-use counts**; password verification uses **OPAQUE/PAKE or an online authorized admission helper** so rate-limits are not bypassable by an offline-copyable verifier; final admission requires an authorized MLS add/commit or an expiring Welcome — the link alone is not sufficient.
- **Retention model (locked, v1.2):** per-author cached window; **default 7 days**; presets 1 h / 24 h / 7 d / 30 d / 90 d + custom + **explicit warned "unlimited/never-lock" opt-in**; **lock-not-vanish**; **shorten = retroactive, lengthen = future-only**; archival (>window) served live by the author's **membership-proven, rate-limited** device(s) (author-as-KMS); presence-leak mitigation is a **blocking** design item (membership-proof at relevant epoch + rate-limit + optional decoy) scoped to the non-member live-key authorization/decryptability oracle beyond generic transport reachability. Multi-device: any of the author's online devices can serve archival keys.
- **Build/runtime:** Tauri 2; Rust hosts OpenMLS natively; **Android voice is an explicit track** (webview Encoded-Transform is the primary at-risk media path now that Web is out → webrtc-rs/native contingency). `transport` trait retained (QUIC native; web impl deferred).
- **v1 platforms:** Windows, macOS, Linux, Android.

## Non-Goals (v1)
- **Web platform** (deferred to a later track; the `transport` trait + WASM-friendly crate choices keep the door open).
- Video calls and screen sharing (→ v2).
- Granular per-channel roles/permissions (→ v2; v1 = owner/admin/member).
- iOS (→ v2).
- Username directory / search-by-name.
- Guaranteed permanent availability of history when authors are offline beyond their retention window — by design; messages **lock** (not vanish). Optional volunteer relays improve but do not guarantee availability.
- **Metadata anonymity / traffic-analysis resistance** (timing/endpoint-churn correlation) — acknowledged, out of v1 scope.
- Making content unrecoverable from a **malicious recipient** who already decrypted (retention/shred/tombstones are cooperative controls only).
- True un-send of already-decrypted messages.

## Acceptance Criteria
- [ ] Identity + multi-device: a user generates a keypair; a friend-code/QR establishes a verified E2E DM with **explicit MITM safety-number verification** and no directory server.
- [ ] **Multi-device:** a user adds a second device via existing-device authorization; the new device joins as its own MLS leaf, syncs history, and other members are notified of the new device. Removing/rotating a device evicts the leaf and rekeys.
- [ ] Invite link admission: links **expire / are revocable / honor max-use**; final admission requires an authorized MLS add/commit or expiring Welcome (link alone insufficient); password gate uses OPAQUE/PAKE or an online authorized admission helper for real rate-limits and is independent of the room secret.
- [ ] Text is OpenMLS-encrypted; each user is authoritative only for its own sent author-log; recipients may cache bounded received ciphertext plus eligible cached keys within retention policy; authors broadcast their author-log over the overlay on coming online.
- [ ] A 12–16 person server syncs text history across members via the gossip overlay with **ordered commit delivery + epoch reconciliation**.
- [ ] **AC-MLS-FORK:** delayed/offline members **detect** divergence and **never silently accept** a forked/downgraded/replayed history, **and repair deterministically** — all honest members converge to one valid MLS state with equal confirmation tags via re-add/reboot/external-commit repair. Invalid divergent MLS commits are not replayed; only still-valid application-level governance/text events are re-proposed and revalidated against the repaired winner epoch. Oracle = repaired MLS state converges and accepted replayed app events remain valid under current membership/authority.
- [ ] **AC-GOV:** role/invite/ban/retention-policy changes are signed, ordered, epoch-bound application events; an unauthorized or out-of-epoch admin action is rejected; concurrent offline admin changes **resolve deterministically via the canonical comparator evaluated against the last common accepted tree and repaired winner epoch**; a **removed admin cannot win a same-epoch race**. Oracle = two conflicting offline admin events yield the same resolved state on every honest client after repair/revalidation.
- [ ] Group voice works in a channel; with >8 participants the adaptive overlay carries media (≥1 relay hop, trees ≤3 deep), <150 ms mouth-to-ear direct + ≤+40 ms/hop.
- [ ] Relay drop mid-call re-routes live (≤3 s convergence), re-delivers lost packets (no permanent loss), audible gap ≤200 ms.
- [ ] A relay provably cannot decrypt forwarded media (SFrame); an active relay that injects/drops/replays is detected and rejected (anti-replay window); malicious-member media impersonation is rejected by per-sender/per-device keys or MLS-signed `KID → MLS leaf/device` binding.
- [ ] **Store-and-forward is opportunistic:** a message to an offline recipient is held as ciphertext by relays (or an optional volunteer relay) and delivered+decrypted on return within the author's **current effective** window; if the author shortens the window below the queued message age before delivery, the message locks rather than decrypting from cache; the UX states delivery is **not guaranteed** without stable peers/relays.
- [ ] Retention: with the default 7-day window, ≤7-day messages decrypt offline; >7-day messages are **locked placeholders that do not vanish**, re-decrypting only while one of the author's devices is online.
- [ ] Retention config: presets (1 h/24 h/7 d/30 d/90 d) + custom + an **explicit warned "unlimited" opt-in**; **shortening re-locks existing messages sooner; lengthening applies only to future messages**.
- [ ] **AC-PRESENCE:** archival-key (live-key) requests are gated by **membership proof at the relevant epoch** (verified **locally** from a signed group-state credential — no online lookup that would itself leak presence) + rate-limited; **a non-member cannot obtain a live-key authorization/decryptability success-failure oracle beyond generic transport reachability** (optional decoy responses). Scope: non-member live-key oracle only — a malicious member can still infer liveness, and generic timing/reachability metadata remains out of scope (residual noted in Threat Model).
- [ ] Crypto-shred (cooperative): author key destruction renders undelivered + archival messages unreadable on **honest online** clients/relays; tombstones purge delivered copies; **cross-device shred is best-effort** — an offline own device retains keys until reconnect and must check tombstones before serving. UX: "deleted on online devices now; pending on offline" — never "deleted everywhere," and cannot remove screenshots/backups/exports/modified clients.
- [ ] **AC-SHRED-PERSIST:** negative tests prove no recoverable plaintext/keys remain in local SQLite/WAL or enumerated key stores after shred (native zeroization).
- [ ] Account destruction wipes local data and renders all of that user's distributed ciphertext undecryptable on honest **online** nodes (an offline own device retains keys until it reconnects and syncs the tombstone). **Backup/restore is account-continuity only** (identity + device-set + room membership; not an archival content-key vault), so a restore cannot resurrect shredded/expired content; recovery requires an existing authorized device, recovery code, or sealed account-continuity backup, and no-material lost-passphrase/account recovery is non-recoverable; a compromised device is rotated out (leaf eviction + rekey) with identity preserved.
- [ ] Connectivity falls back STUN → relay-overlay → TURN (ciphertext only); owner can override STUN/TURN endpoints.
- [ ] Builds and runs natively on Windows, macOS, Linux, and Android from the Tauri toolchain. **Android voice path verified** (webview Encoded-Transform or webrtc-rs/native contingency).
- [ ] Android incoming-call wake via content-free FCM (no message/identity content).
- [ ] Basic roles enforced (owner/admin/member) via the governance log.
- [ ] **AC-METADATA-MATRIX:** infrastructure metadata exposure matches the documented matrix; pcap proves no central content egress, relay ciphertext-only, content-free push, signaling no-linkage-at-rest.
- [ ] Abuse mitigations present: invite-flood/spam rate-limits, Sybil-resistance posture documented, relay-freeloading accounted for in ranking.

## Technical Context (technology choices)
- **App framework:** Tauri 2 (Rust + React); native Win/Mac/Linux/Android. Web deferred (WASM-friendly crate choices + `transport` trait retained to keep it cheap later).
- **Group crypto:** OpenMLS (MIT, RFC 9420) + a custom delivery/ordering/Welcome/catch-up/rejoin + fork-detection layer.
- **Multi-device:** per-device MLS leaves under one identity; existing-device-authorized pairing (QR); device-change transparency notices; backup/restore; compromised-device rotation.
- **Governance + ordering:** signed, ordered, epoch-bound room-state application log (roles/invites/bans/retention policy), ordered by the **canonical comparator** (epoch → committer/author leaf index in the last common accepted tree → signed content hash) shared with the MLS delivery layer and device-leaf eviction; fork repair = comparator selects repair target/coordinator, losers rejoin via re-add/reboot/external-commit, and only still-valid application events are re-proposed/revalidated.
- **Media:** WebRTC (webview RTCPeerConnection + Encoded Transform; webrtc-rs harness/contingency, primary fallback for Android); SFrame (RFC 9605) keyed from MLS exporter; keys in Rust; per-sender/per-device media keys or MLS-signed `KID → MLS leaf/device` binding provide sender authentication.
- **Transport:** `transport` trait; native = `quinn`/QUIC. (Web/DataChannel impl deferred with the Web track.)
- **Relay overlay:** adaptive ALM; ping/stability/proximity/energy ranking; failover + per-packet re-delivery; integrity + anti-replay; gossip for text; opportunistic store-and-forward + optional content-blind volunteer relays.
- **Retention/deletion:** per-author content-key lifecycle (default 7 d; presets + custom + warned-unlimited); lock-not-vanish; shorten-retroactive/lengthen-future; author-as-KMS (membership-proven, rate-limited, decoy); cooperative crypto-shred with native zeroization + SQLite/WAL negative tests.
- **Admission:** expiring/revocable/max-use invites; OPAQUE/PAKE or online authorized-helper password gate for real rate-limits; MLS-commit/Welcome final admission.
- **Signaling/rendezvous:** content-blind, zero linkage at rest. **Mobile wake:** content-free FCM.
- **Storage:** local-only per-author logs; OS-keychain-wrapped at-rest keystore (native); cross-device sync of own logs/keys; relays hold transient ciphertext only.

## Ontology (Key Entities)
| Entity | Type | Fields | Relationships |
|--------|------|--------|---------------|
| User / IdentityKey | core domain | identity keypair, friend-code/QR fingerprint, **safety-number**, display name, retention-policy (default 7d; presets/custom/unlimited) | owns Devices; authors Messages; member of Servers |
| Device | core domain | device key, **MLS leaf**, current endpoint, platform (desktop/Android) | belongs to User; one MLS leaf per device |
| Message | core domain | content, author, timestamp, epoch/content-key ref, age-tier (cached/locked) | authored by User; part of History |
| History | core domain | per-author log (multi-device merged) | synced via RelayOverlay + ordering layer; subject to RetentionWindow |
| Room | core domain | room name, room secret (in invite link), optional password, **governance log** | is a Server or a DM |
| GovernanceLog | core domain | signed ordered epoch-bound events (role/invite/ban/retention changes) | governs Room state |
| InviteLink | supporting | room secret, signaling endpoint, **expiry, revocation, max-use** | grants entry to Room (via MLS commit/Welcome) |
| EncryptionKey / EpochKey / SFrameMediaKey | core domain | MLS group/epoch/content keys, exporter-derived media key | secures Messages/Media |
| SignalingServer / STUN / TURN | external system | endpoints (default + custom); TURN relays ciphertext | NAT traversal / discovery; content-blind |
| RelayPeer | core domain | capacity (~8), ping/stability/energy rank | relays for peers in OverlayTopology |
| VolunteerStorageRelay | external system (optional) | transient ciphertext, TTL; content-blind | improves opportunistic store-and-forward |
| OverlayTopology | core domain | adaptive tree, hop depth, failover routes | composed of RelayPeers |
| Transport | supporting | kind (QUIC native; web deferred), datagram+stream | carries overlay + signaling |
| RetentionWindow | supporting | per-author; default 7d; presets/custom/unlimited; lock-not-vanish; shorten-retro/lengthen-future | governs Message decryptability tier |
| StoreForwardQueue | core domain | transient ciphertext, TTL; membership-gated | held by RelayPeers/VolunteerRelays for offline recipients |
| Member / Role | supporting | owner/admin/member (governance-log authored) | links User to Server |

## Ontology Convergence
| Round | Entity Count | New | Changed | Stable | Stability Ratio |
|-------|-------------|-----|---------|--------|----------------|
| 12 (original final) | 18 | 0 | 0 | 18 | 100% |
| v1.1 amend | 19 | 1 (Transport) | 3 | 18 | post-interview |
| v1.2 amend | 21 | 2 (GovernanceLog, VolunteerStorageRelay) | 4 (Device, User, RetentionWindow, InviteLink) | 17 | post-interview |

## Interview Transcript
<details>
<summary>Original Full Q&A (12 rounds) — historical record</summary>

### Round 0 — Topology Enumeration
**Q:** Confirm 5 top-level components + crypto spine. **A:** "Looks right (5 + crypto spine)."

### Round 1 — Crypto goal
**A:** "All three, ranked" (deletion-control, always-available history, compromise-containment). **Ambiguity:** 77%

### Round 2 — Offline history
**A:** "Tiered by recency." *(v1.1: lock, not vanish.)* **Ambiguity:** 71%

### Round 3 — Scale
**A:** Adaptive peer-relay overlay ("P2P SFU"). **Ambiguity:** 64%

### Round 4 — CONTRARIAN: relay phasing
**A:** "Relay overlay is v1-critical." **Ambiguity:** 59%

### Round 5 — Identity model
**A:** "Friend codes / QR (out-of-band)." **Ambiguity:** 55%

### Round 6 — SIMPLIFIER: v1 definition
**A:** "Full Discord-clone v1." **Ambiguity:** 49%

### Round 7 — Infra ownership
**A:** Federated + relay-first; STUN→overlay→TURN; group-custom endpoints. **Ambiguity:** 43%

### Round 8 — Crypto mechanism
**A:** "Want a deeper comparison first." **Ambiguity:** 43%

### Round 9 — Crypto lock
**A:** "Lock MLS/OpenMLS + crypto-shred + SFrame." **Ambiguity:** 38%

### Round 10 — Platforms
**A:** "Desktop + Android v1" (iOS → v2). *(v1.1 added Web; v1.2 removed Web from v1.)* **Ambiguity:** 33%

### Round 11 — v1 acceptance set
**A:** Only "Offline store-and-forward" as a heavy gate. **Ambiguity:** 25%

### Round 12 — Retention model
**A:** "Fixed 30d window, then lock." *(v1.1: per-author 7d configurable; v1.2: presets+custom+warned-unlimited, shorten-retro/lengthen-future.)* **Ambiguity:** 17.8% — PASSED

</details>

<details>
<summary>Review-incorporation Q&A (3 rounds, 2026-05-24)</summary>

### Round R0 — Decision topology
**Q:** Confirm 6 decision clusters (scope; web-trust; retention-bounds; claims-precision; room-state+MLS; availability+identity+recovery) and the you-decide (1-3) vs default-and-confirm (4-6) split. **A:** "Topology looks right — proceed."

### Round R1 — v1 scope & phasing
**Q:** Keep overlay+Web+SF+Android all in v1, stage them, or cut Web? **A:** "Only web out of the picture" — keep overlay, retention, MLS, group voice, SFrame, store-and-forward, Android; **remove Web from v1**. **Ambiguity:** ~17%

### Round R2 — Retention bounds + policy semantics
**Q (bounds):** no-ceiling vs presets+max vs presets+custom+warned-unlimited? **A:** presets + custom + explicit warned "unlimited" opt-in.
**Q (semantics):** future-only vs retroactive vs shorten-retro/lengthen-future? **A:** shorten = retroactive, lengthen = future-only. **Ambiguity:** ~9%

### Round R3 — Clusters 4-6 confirmation
**Q:** Adopt claims-precision + governance-log/MLS-robustness + availability/identity/recovery defaults; multi-device single-v1 vs in-v1; volunteer-relays optional vs core? **A:** "Adopt, but multi-device IN v1" (volunteer relays remain optional). **Ambiguity:** ~6% — PASSED

</details>

## Research References
- MLS / RFC 9420: https://datatracker.ietf.org/doc/rfc9420/
- OpenMLS (MIT, Rust): https://github.com/openmls/openmls
- SFrame / RFC 9605: https://datatracker.ietf.org/doc/rfc9605/
- WebRTC Encoded Transform: https://www.w3.org/TR/webrtc-encoded-transform/
- Tauri 2: https://v2.tauri.app/
- CRDTs for decentralized chat sync: https://unzip.dev/0x018-crdts/
- Crypto-shredding overview: https://grokipedia.com/page/Crypto-shredding
