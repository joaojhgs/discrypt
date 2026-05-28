# Phase 7 UX and E2E hardening review

G008 implements the final v1 command/UI skeleton and E2E acceptance harness wiring.
It provides deterministic command contracts, real Tauri command registration behind the `tauri-runtime` feature, and a native-shell smoke while avoiding unsupported product scope.

## Implementation map

- `crates/core/src/lib.rs`
  - `AppSnapshot` is the serialized contract consumed by Tauri commands and React.
  - stable schema-versioned snapshot covers friend verification, device management, Discord-style
    server/channel navigation, voice status, invite admission, retention settings,
    connectivity/push posture, and mandatory honest deletion/metadata copy.
  - `verify_safety_number` keeps the expected safety number backend-owned and requires an exact out-of-band user confirmation.
- `apps/desktop/src-tauri/src/lib.rs`
  - Tauri command facade (`app_snapshot`, `verify_safety_number`,
    `deletion_warning`, `metadata_warning`, `command_health`).
  - commands are annotated with `#[tauri::command]` and registered through
    `tauri::generate_handler!` when the `tauri-runtime` feature is enabled; the default
    build keeps CI lightweight.
- `apps/desktop/src-tauri/src/main.rs` + `tauri.conf.json`
  - when `tauri-runtime` is enabled, the binary starts `tauri::Builder`, attaches the
    generated invoke handler, and runs the native shell; otherwise it remains a
    dependency-light command smoke for CI.
- `apps/ui/src/commands.ts`
  - typed command bridge that calls `window.__TAURI__.core.invoke` when available, exposes the safety-number verification command, fails visibly on invoke errors, and uses deterministic preview data only when no Tauri API exists.
- `apps/ui/src/main.tsx` + `styles.css`
  - Discord-style shell with server rail, channel sidebar, voice room, verified
    friend panel, invite flow, device management, retention settings, connectivity,
    and honest security guarantees.
- `harness/multinode/src/lib.rs`
  - `ux_e2e_hardening_smoke` proves the command surface, UI model, honest copy, and
    all earlier deterministic phase smokes stay wired at the final gate.

## Acceptance coverage

- AC1/AC2: friend safety-number verification and two authorized device rows are in
  the command snapshot and UI model.
- AC3/AC16: invite admission copy covers expiry/revoke/max-use posture, password
  helper/PAKE posture, and final MLS Welcome/add requirement; server role is shown.
- AC10/AC10b/AC11/AC-RECOVERY: retention presets include warned unlimited,
  transition semantics are surfaced, and deletion copy uses the approved offline-device
  caveat.
- AC13/AC15/AC18/AC-METADATA: connectivity, content-free push, and metadata posture
  are surfaced in the React shell and final harness smoke.
- AC14 environment-permitted builds: Rust host/Android checks and React build are
  included in the final verification gate. Full OS-native Tauri packaging remains
  environment/toolchain-dependent.

## Production-hardening notes

- The command facade keeps the heavy Tauri runtime behind the `tauri-runtime` feature. Production packaging should enable that feature and preserve the serialized field names. In this container, checking that feature is blocked by missing Linux GTK/WebKit pkg-config development packages (`glib`, `gio`, `gobject`, `atk`, `gdk`, WebKit) and unavailable root installation; the source-level builder/invoke-handler wiring is checked in for environments with those OS packages.
- The React shell is a deterministic UX skeleton, not a complete Discord clone. It
  proves required flows and copy gates without adding unsupported video/iOS/web scope.
- Final verification must still include cleanup, full workspace checks, UI build/audit,
  and independent code/architecture review before the aggregate ultragoal is complete.
