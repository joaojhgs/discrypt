#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND=noninteractive
export CARGO_HOME=${CARGO_HOME:-/work/.cargo-home}
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-/work/discrypt/target-remote-tauri}
export NPM_CONFIG_CACHE=${NPM_CONFIG_CACHE:-/work/.npm-cache}
export PATH="$CARGO_HOME/bin:$PATH"
export WEBKIT_DISABLE_COMPOSITING_MODE=1
export WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1
export LIBGL_ALWAYS_SOFTWARE=1
export NO_AT_BRIDGE=1
export XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-/tmp/discrypt-runtime}
export DISCRYPT_G012_TAURI_DRIVER=${DISCRYPT_G012_TAURI_DRIVER:-$CARGO_HOME/bin/tauri-driver}
export DISCRYPT_G012_NATIVE_WEBDRIVER=${DISCRYPT_G012_NATIVE_WEBDRIVER:-/usr/bin/WebKitWebDriver}
export DISCRYPT_G012_APP_BINARY=${DISCRYPT_G012_APP_BINARY:-/work/discrypt/target-remote-tauri/debug/discrypt-desktop}
export DISCRYPT_G012_REQUIRE_NATIVE_VOICE=${DISCRYPT_G012_REQUIRE_NATIVE_VOICE:-1}
export DISCRYPT_G012_WEBDRIVER_SKIP_BUILD=1

mkdir -p "$CARGO_HOME" "$CARGO_TARGET_DIR" "$NPM_CONFIG_CACHE" "$XDG_RUNTIME_DIR" remote-evidence
chmod 700 "$XDG_RUNTIME_DIR"

apt-get update
apt-get install -y --no-install-recommends \
  nodejs npm xvfb xauth dbus-x11 \
  pulseaudio pulseaudio-utils alsa-utils gstreamer1.0-pulseaudio \
  webkit2gtk-driver libwebkit2gtk-4.1-dev libgtk-3-dev \
  libayatana-appindicator3-dev librsvg2-dev patchelf pkg-config \
  ca-certificates curl

npm --prefix apps/ui ci
npm --prefix apps/ui run typecheck
npm --prefix apps/ui run build
cargo build -p discrypt-desktop --features tauri-runtime,local-dev,production-media,mqtt-adapter,nostr-adapter,ipfs-pubsub-adapter,discrypt-quic-rendezvous-adapter
if [ ! -x "$DISCRYPT_G012_TAURI_DRIVER" ]; then
  cargo install tauri-driver --locked || cargo install tauri-driver
fi

pulseaudio --check || pulseaudio --start --exit-idle-time=-1 || true
pactl load-module module-null-sink sink_name=discrypt_sink sink_properties=device.description=Discrypt_Sink >/dev/null 2>&1 || true
pactl set-default-sink discrypt_sink >/dev/null 2>&1 || true
pactl set-default-source discrypt_sink.monitor >/dev/null 2>&1 || true
pactl info > remote-evidence/g012-docker-pulse-info.log 2>&1 || true
pactl list short sources > remote-evidence/g012-docker-pulse-sources.log 2>&1 || true

npm --prefix apps/ui run dev -- --host 127.0.0.1 --port 1420 --strictPort > remote-evidence/g012-vite.log 2>&1 &
VITE_PID=$!
cleanup() {
  kill "$VITE_PID" >/dev/null 2>&1 || true
}
trap cleanup EXIT
for _ in $(seq 1 80); do
  if curl -fsS http://127.0.0.1:1420 >/dev/null 2>&1; then
    break
  fi
  sleep 0.25
done
curl -fsS http://127.0.0.1:1420 >/dev/null

ARTIFACT_DIR=${DISCRYPT_G012_ARTIFACT_DIR:-target/g012-e2e/remote-docker-gui-audio-$(date -u +%Y%m%dT%H%M%SZ)}
dbus-run-session -- xvfb-run -a \
  node scripts/g012-tauri-webdriver-integrated.mjs \
    --run \
    --skip-build \
    --require-native-voice \
    --artifact-dir "$ARTIFACT_DIR"
