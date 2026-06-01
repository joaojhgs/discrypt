# Discrypt UI — Production Gap Analysis

**Date:** 2026-05-30  
**Status:** Implementation in progress

## Executive Summary

The backend runtime is substantially complete. Six production user stories across signaling (MQTT/Nostr), role-split text transport, group/DM receipt, relay overlay, and media-frame signaling all have passing test evidence. The gap is entirely on the frontend: developer diagnostic panels (always-on amber banner, transport debug strip, workflow tab bar) dominate the visible surface and block any real user interaction. Fixing the UI requires no new backend APIs — only routing the existing command surface through natural chat UX patterns.

---

## Production Status Matrix

| Area | Backend | Frontend | Gap |
|------|---------|----------|-----|
| Profile creation / recovery | ✅ Tauri command | ✅ FirstRunPanel | None |
| Group creation | ✅ `create_group` command | ⚠️ Gated behind `WorkflowNav` "Groups" tab | WorkflowNav must be removed; sidebar Create button should open modal |
| Join group via invite | ✅ `join_group` command | ⚠️ Gated behind "Invites" tab | Same — use sidebar Join |
| Create invite | ✅ `create_invite` command | ✅ TopBar button | None |
| Channel creation | ✅ `create_channel` command | ⚠️ Gated behind "Text" tab | Should open inline from channel sidebar "+" |
| Text channel messaging | ✅ `send_text_channel_message` | ⚠️ Only reachable via "Text" tab; no auto transport | Need auto transport attach + channel panel as default view |
| DM messaging | ✅ `start_dm` + `send_dm` | ⚠️ Only reachable via "DMs" tab; no auto transport | Need auto transport attach; DM panel should open from sidebar |
| Text delivery to remote peer | ✅ Role-split runtime (MQTT/Nostr proven) | ❌ Manual: user must enter peer IDs, click Offerer/Answerer buttons | Auto-attach transport on DM/channel open |
| Real-time message push | ❌ Backend emits no Tauri events; UI polls every 5 s | ❌ 5 s polling loop | Backend `emit` + frontend `listen` event subscription |
| Voice join/mute/leave | ✅ `join_voice_channel`, `mute_voice`, `leave_voice` | ⚠️ Only reachable via "Voice" tab | Route via sidebar voice channel click |
| Voice audio (mic → speaker) | ❌ No audio track negotiation | ❌ No `getUserMedia` / `addTrack` / audio element | Full WebRTC audio pipeline |
| Signaling config UI | ❌ No per-group adapter picker | ❌ Group create has no adapter/STUN/TURN fields | Add config step to group creation wizard |
| STUN/TURN config | ❌ Hardcoded Google STUN only | ❌ No UI | ICE server config in group creation |
| Relay overlay | ✅ 34/34 tests passing | Not yet wired to UI | Wire relay toggle in group settings |

---

## Priority Stack

### P0 — Remove harness appearance (no backend changes needed)

The three developer panels that make the app look like a test harness:

| Component | Current | Fix |
|-----------|---------|-----|
| `RuntimeModeBanner` | Always rendered at line 924 — amber "local-dev / harness mode" warning | Move into InspectorRail only; never render in main content area |
| `TransportStatusStrip` | Always rendered at line 935 — exposes peer ID inputs and probe buttons | Move into InspectorRail only; never render in main content area |
| `WorkflowNav` | Always rendered at line 948 — tab bar "Setup/DMs/Text/Voice/Invites/Groups" | Remove entirely; ChannelSidebar already drives all navigation |

**Impact:** Immediately makes the app look like a Discord-style chat client. ChannelSidebar already has buttons for all sections. No backend changes required.

### P1 — Automatic text delivery

| Gap | Fix |
|-----|-----|
| Transport attach is manual (user must enter peer IDs + click button) | Auto-call `attach_text_control_transport_runtime` as answerer when a DM or channel is opened; auto-start as offerer when send is clicked with no runtime attached |
| Messages push via 5 s polling | Add Tauri `emit("new_message", ...)` in backend `send_text_channel_message` handler; frontend `listen("new_message", ...)` subscription |

### P2 — Voice audio

| Gap | Fix |
|-----|-----|
| No audio track negotiation | `getUserMedia({audio: true})` → `peerConnection.addTrack()` → new offer/answer exchange including audio codec |
| No remote audio output | Remote track → `<audio autoPlay>` element per participant |
| No speaking indicator | `RTCRtpReceiver.getSynchronizationSources()` audio level → `speaking` flag |
| Mute | `audioTrack.enabled = false` |

### P3 — Signaling configuration UI

| Gap | Fix |
|-----|-----|
| Group creation has no adapter picker | Add step 2 to CreateGroupPanel: adapter radio (MQTT / Nostr / IPFS), optional custom endpoint |
| No STUN/TURN config | ICE server text fields in group creation (default: stun:stun.l.google.com:19302) |
| No per-group settings panel | Settings icon → modal with adapter + ICE fields for existing groups |

---

## Open Backend Gates (not blocking UI wiring)

These are true production gaps but do NOT block the UI from working for local/LAN users:

| Gate | Status | Workaround |
|------|--------|------------|
| Real-time message push (Tauri events) | ❌ Missing | 5 s poll works for MVP |
| Credentialed TURN server | ❌ Not configured | Direct ICE works on same network |
| IPFS public rendezvous | ⚠️ Local-only proven | MQTT/Nostr work for signaling |
| QUIC deployed endpoint | ❌ Not deployed | DataChannel over MQTT/Nostr works |
| Hardware audio (mic → speaker) | ❌ Cannot test without hardware | Mute/join/leave state works |

---

## Implementation Order

1. **P0** — UI harness cleanup (this session, no backend touches)  
2. **P1a** — Channel/DM panels as default view (sidebar click → panel, no workflow nav required)  
3. **P1b** — Auto-attach transport on DM/channel open  
4. **P1c** — Tauri event push for new messages (backend + frontend)  
5. **P2** — Voice audio pipeline  
6. **P3** — Signaling config UI in group creation  
