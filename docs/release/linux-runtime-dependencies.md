# Linux runtime dependencies for discrypt desktop packages

This document separates **developer/build-host dependencies** from **end-user runtime dependencies** for the Linux artifacts produced by `npm --prefix apps/ui run release:linux`.

## Release artifacts

The Linux release script produces the package formats enabled by `apps/desktop/src-tauri/tauri.conf.json`:

| Artifact | Intended users | Runtime dependency behavior | End-user install notes |
| --- | --- | --- | --- |
| `.deb` | Debian, Ubuntu, Linux Mint, and compatible apt-based distributions | Native package. The Tauri Debian bundle declares runtime libraries such as WebKitGTK 4.1 and GTK 3 for the distribution package manager. | Install with `apt install ./discrypt_*.deb` or `dpkg -i` followed by `apt -f install` if dependency resolution is needed. End users should not install `-dev` packages. |
| `.rpm` | Fedora, RHEL-compatible, openSUSE/RPM-based distributions | Native package. Runtime libraries are resolved by the RPM package manager. Minimum glibc is determined by the build base system, so build on the oldest supported baseline. | Install with `dnf install ./discrypt-*.rpm`, `zypper install ./discrypt-*.rpm`, or equivalent. End users should not install `*-devel` packages. |
| `.AppImage` | Portable Linux distribution for users outside native package families | Bundles application files and most libraries, but host compatibility still depends on the build baseline and core runtime environment. | Mark executable (`chmod +x discrypt*.AppImage`) and run it. Prefer AppImage when the distro cannot use `.deb` or `.rpm`; if the host blocks AppImage execution, use the native package for that distro. |

## Runtime libraries users may need

Discrypt is a Tauri v2 desktop app. The UI is rendered by the platform WebView stack, so native `.deb`/`.rpm` packages must resolve runtime WebKitGTK/GTK libraries. Development headers are only needed on build machines.

Common runtime dependency names by package family:

| Package family | Runtime packages | Build-only packages that should **not** be required on user machines |
| --- | --- | --- |
| Debian/Ubuntu | `libwebkit2gtk-4.1-0`, `libgtk-3-0`, plus `libappindicator3-1` only if tray integration is enabled in the future | `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `build-essential`, `pkg-config` |
| Fedora/RHEL/openSUSE | WebKitGTK 4.1 runtime package, GTK 3 runtime package, and appindicator/Ayatana runtime only if tray integration is enabled in the future | `webkit2gtk4.1-devel`, GTK development headers, compiler toolchains, `pkg-config` |
| AppImage | Bundled by the AppImage where practical; host still supplies kernel/glibc compatibility and desktop integration behavior | WebKitGTK/GTK development headers should not be installed by end users to run the AppImage |

## Build-host baseline

Build hosts need the development packages because the Tauri/Wry desktop backend links against native WebKitGTK and GTK. End users running a built package should receive runtime libraries through `.deb`/`.rpm` dependency metadata or the AppImage bundle.

Use an older supported Linux baseline for release builds. Building on a newer distro can raise the required glibc version for the produced binary, which may prevent older supported systems from launching the app.

Recommended release validation order:

1. Build with `npm --prefix apps/ui run release:linux` on the chosen Linux baseline.
2. Inspect `.deb` metadata with `dpkg-deb -I target/release/bundle/deb/*.deb` and verify runtime dependencies only.
3. Inspect `.rpm` metadata with `rpm -qpR target/release/bundle/rpm/*.rpm` and verify runtime dependencies only.
4. Smoke-run the AppImage on the oldest supported baseline and one current distro.
5. Confirm no end-user instructions mention development headers or compilers.

## Current honesty boundary

The release script is present and dry-run validated. Actual package build/install smoke is tracked separately by the packaging verification goal; until that goal passes, this document is the dependency contract, not proof that every artifact was installed on every target distribution.

## Sources used for dependency policy

- Tauri v2 Linux prerequisites distinguish Linux build dependencies such as WebKitGTK/GTK development packages from runtime usage.
- Tauri v2 Debian packaging documentation states the generated Debian package includes icons/desktop metadata and declares runtime dependencies including WebKitGTK 4.1 and GTK 3, plus appindicator when the app uses a tray.
- Tauri v2 AppImage documentation states AppImage bundles dependencies/files and recommends building on an old enough baseline because newer build hosts can increase runtime compatibility requirements.
