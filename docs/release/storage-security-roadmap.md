# Discrypt storage-security roadmap

Date: 2026-06-04

This roadmap captures the production-storage decisions and remaining security/UX work from the full Discrypt production-readiness session.

## Current decision: explicit storage choice before account setup

Discrypt must not silently create a new vault/keyring entry when local encrypted state cannot be opened. Before account setup, production Linux builds now route users through a storage-security wizard:

1. **Use OS keyring if available**
   - Uses the desktop Secret Service/KWallet/GNOME Keyring path.
   - Best UX because the OS may unlock it with the login session.
   - Security downside: it trusts the logged-in OS/session keyring boundary.
2. **Use Discrypt password vault**
   - Uses the production encrypted vault with a key derived from the user password.
   - Worse UX because the password is required on every app startup.
   - Security upside: the local Discrypt state has an app-level secret separate from the OS keyring.

If either mode fails to unlock existing app state, Discrypt must error out and preserve the existing files. Recovery and migration are future work; the app must not overwrite an old vault/keyring path with a replacement secret just to continue.

## Production-storage follow-ups

- [ ] Add a guided recovery flow for broken OS keyring access without replacing encrypted app state.
- [ ] Add password-vault recovery guidance for users who know the password but moved files between machines.
- [ ] Add a storage-mode migration flow: keyring → password vault and password vault → keyring, with explicit re-encryption and rollback evidence.
- [ ] Add optional password-cache duration controls after the always-required startup password behavior is stable.
- [ ] Add platform-native production keychain implementations and tests for macOS, Windows, Android, and iOS.
- [ ] Simplify Linux package dependencies if the password-vault path becomes sufficient for distributions without Secret Service packages.
- [ ] Add a live Secret Service/KWallet/GNOME Keyring E2E gate that exercises prompt/unlock behavior outside containers.

## UI/UX follow-ups from the session

- [ ] Continue Discord-like refinement: text remains the main full-height surface, voice members remain under voice channels, and global audio controls live in the sidebar/config modal.
- [ ] Keep main chat free of persistent banners; use shadcn-style notifications/toasts and console errors for command failures.
- [ ] Keep group invite creation in group context menus/modals; keep invite acceptance in the launcher/join flow for the joining user.
- [ ] Keep direct messages as first-class left-rail conversations, not group channels.
- [ ] Finish any remaining modal animation and context-menu polish using shared components.

## Networking / E2E follow-ups from the session

- [ ] Repeat split-machine public signaling tests after each release artifact rebuild.
- [ ] Keep MQTT and Nostr public signaling profiles available in normal dev/runtime/release builds.
- [ ] Maintain true provider-derived runtime attachment: no manual peer IDs in production UI.
- [ ] Preserve native Rust voice proof coverage while expanding real microphone/audio E2E where automation permits.
- [ ] Add remote-machine release smoke scripts for the provided SSH host once artifact distribution is stable.

## Release / production follow-ups from the session

- [ ] Keep `.deb`, `.rpm`, and `.AppImage` release smoke checks current.
- [ ] Continue maintaining the no-placeholder/no-honesty-copy production gates so normal UI has no test/proof copy.
- [ ] Add production update/rollback signing and verification before public distribution.
- [ ] Keep Android as roadmap until Tauri mobile storage, audio permission, and network E2E are proven.
