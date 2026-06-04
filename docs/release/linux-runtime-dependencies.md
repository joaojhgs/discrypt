# Linux runtime dependencies for discrypt desktop packages

This document separates **developer/build-host dependencies** from **end-user runtime dependencies** for the Linux artifacts produced by `npm --prefix apps/ui run release:linux`.

## Release artifacts

The Linux release script produces the package formats enabled by `apps/desktop/src-tauri/tauri.release.conf.json`:

| Artifact | Intended users | Runtime dependency behavior | End-user install notes |
| --- | --- | --- | --- |
| `.deb` | Debian, Ubuntu, Linux Mint, and compatible apt-based distributions | Native package. The Tauri Debian bundle declares runtime libraries such as WebKitGTK 4.1 and GTK 3 plus Linux Secret Service/keychain runtime packages for users who select OS-keyring storage. | Install with `apt install ./discrypt_*.deb` or `dpkg -i` followed by `apt -f install` if dependency resolution is needed. End users should not install `-dev` packages. |
| `.rpm` | Fedora, RHEL-compatible, openSUSE/RPM-based distributions | Native package. Runtime libraries and optional Secret Service provider packages are resolved by the RPM package manager. Minimum glibc is determined by the build base system, so build on the oldest supported baseline. | Install with `dnf install ./discrypt-*.rpm`, `zypper install ./discrypt-*.rpm`, or equivalent. End users should not install `*-devel` packages. |
| `.AppImage` | Portable Linux distribution for users outside native package families | Bundles application files and most libraries, but host compatibility still depends on the build baseline, the build baseline and desktop integration behavior; a Secret Service/keyring provider is only required for users who choose OS-keyring storage. | Mark executable (`chmod +x discrypt*.AppImage`) and run it. Prefer AppImage when the distro cannot use `.deb` or `.rpm`; if the host blocks AppImage execution, use the native package for that distro. |

## Runtime libraries users may need

Discrypt is a Tauri v2 desktop app. The UI is rendered by the platform WebView stack, so native `.deb`/`.rpm` packages must resolve runtime WebKitGTK/GTK libraries. The production Linux storage path can use the OS keychain through the Freedesktop Secret Service API (`org.freedesktop.secrets`) on the desktop default collection, so users who choose OS-keyring storage require a Secret Service provider, user D-Bus session, and login-session keyring unlock integration. KDE/KWallet Secret Service and GNOME Keyring are both acceptable providers; the app must not force a GNOME-only collection when another desktop provider owns `org.freedesktop.secrets`. Development headers are only needed on build machines.

Common runtime dependency names by package family:

| Package family | Runtime packages | Build-only packages that should **not** be required on user machines |
| --- | --- | --- |
| Debian/Ubuntu | `libwebkit2gtk-4.1-0`, `libgtk-3-0`, `gnome-keyring`, `dbus-user-session`, `libpam-gnome-keyring`, plus `libappindicator3-1` only if tray integration is enabled in the future | `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `build-essential`, `pkg-config` |
| Fedora/RHEL/openSUSE | WebKitGTK 4.1 runtime package, GTK 3 runtime package, `gnome-keyring`, and appindicator/Ayatana runtime only if tray integration is enabled in the future | `webkit2gtk4.1-devel`, GTK development headers, compiler toolchains, `pkg-config` |
| AppImage | Bundled by the AppImage where practical; host still supplies kernel/glibc compatibility and desktop integration behavior. A D-Bus user session plus Secret Service provider such as `gnome-keyring` or compatible KWallet Secret Service is needed only for OS-keyring storage; password-vault storage does not require Secret Service. | WebKitGTK/GTK development headers should not be installed by end users to run the AppImage |

## Production storage setup

Linux release builds require an explicit storage-security choice before account setup. The first-run wizard lets the user choose either the native OS keyring or a Discrypt password vault.

- **OS keyring** uses the desktop Secret Service provider (`org.freedesktop.secrets`) through KWallet/GNOME Keyring when available. It is the best UX, but it trusts the logged-in OS/session keyring boundary.
- **Discrypt password vault** stores app DB wrapping keys in an Argon2id/AES-GCM encrypted vault file next to the production app-state envelope. The user password is required on every app startup. It is worse UX, but keeps a separate app-level secret rather than relying on the OS keyring.

If keyring or vault unlock fails, Discrypt errors out and preserves the existing state instead of replacing the old vault/keyring entry. Storage recovery and migration flows are tracked in `docs/release/storage-security-roadmap.md`.

Operators may still provide the vault passphrase externally for controlled deployments:

```sh
DISCRYPT_APPDB_VAULT_PASSPHRASE='use-a-long-unique-device-passphrase' discrypt-desktop
```

The password-vault path requires at least 12 characters. It is not a plaintext wrapping-key fallback and is not compiled into or used by local-dev, harness, test, or non-`production-storage` stores.

## Build-host baseline

Build hosts need the development packages because the Tauri/Wry desktop backend links against native WebKitGTK and GTK. End users running a built package should receive runtime libraries through `.deb`/`.rpm` dependency metadata or the AppImage bundle.

Use an older supported Linux baseline for release builds. Building on a newer distro can raise the required glibc version for the produced binary, which may prevent older supported systems from launching the app.

Recommended release validation order:

1. Build with `npm --prefix apps/ui run release:linux` on the chosen Linux baseline.
2. Inspect `.deb` metadata with `dpkg-deb -I target/release/bundle/deb/*.deb` and verify runtime dependencies only, including `gnome-keyring`, `dbus-user-session`, and `libpam-gnome-keyring` for OS-keyring storage support.
3. Inspect `.rpm` metadata with `rpm -qpR target/release/bundle/rpm/*.rpm` and verify runtime dependencies only, including `gnome-keyring` for OS-keyring storage support.
4. Run `npm --prefix apps/ui run smoke:linux-packages` to install and smoke-launch `.deb` and `.rpm` artifacts in clean Linux containers and smoke-launch the AppImage under Xvfb, plus separate OS-keyring smoke under `dbus-run-session` and an active `org.freedesktop.secrets` Secret Service provider.
5. Smoke-run the AppImage on the oldest supported baseline and one current distro before publishing a public release.
6. Confirm no end-user instructions mention development headers or compilers.

## Current honesty boundary

The release and package-smoke scripts are present. In this repository environment, `npm --prefix apps/ui run release:linux` produced `.deb`, `.rpm`, and `.AppImage` artifacts, and `npm --prefix apps/ui run smoke:linux-packages` is the repeatable gate for clean-container install/launch smoke. This is not a promise that every downstream distribution has been certified; distro certification still requires running the same smoke on each supported release baseline before publishing.

## Sources used for dependency policy

- Tauri v2 Linux prerequisites distinguish Linux build dependencies such as WebKitGTK/GTK development packages from runtime usage.
- Tauri v2 Debian packaging documentation states the generated Debian package includes icons/desktop metadata and declares runtime dependencies including WebKitGTK 4.1 and GTK 3, plus appindicator when the app uses a tray.
- Tauri v2 AppImage documentation states AppImage bundles dependencies/files and recommends building on an old enough baseline because newer build hosts can increase runtime compatibility requirements.
- Discrypt ADR-006 documents Linux `production-storage` using `keyring 3.6.3` with the sync Secret Service provider for OS-keyring mode, and the production password-vault mode for users who choose app-level password unlock.
