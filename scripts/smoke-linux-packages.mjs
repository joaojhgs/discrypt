#!/usr/bin/env node
import { existsSync, readdirSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const bundleRoot = resolve(repoRoot, "target/release/bundle");
const dryRun =
  process.argv.includes("--dry-run") ||
  process.env.DISCRYPT_PACKAGE_SMOKE_DRY_RUN === "1";
const debImage = process.env.DISCRYPT_DEB_SMOKE_IMAGE ?? "mcr.microsoft.com/playwright:v1.58.2-noble";
const rpmImage = process.env.DISCRYPT_RPM_SMOKE_IMAGE ?? "fedora:41";
const appImageSmokeImage =
  process.env.DISCRYPT_APPIMAGE_SMOKE_IMAGE ?? "mcr.microsoft.com/playwright:v1.58.2-noble";

function fail(message) {
  console.error(`smoke-linux-packages: ${message}`);
  process.exit(1);
}

function firstFile(dir, suffix) {
  const fullDir = resolve(bundleRoot, dir);
  if (!existsSync(fullDir)) fail(`missing bundle directory: ${fullDir}`);
  const candidate = readdirSync(fullDir)
    .filter((entry) => entry.endsWith(suffix))
    .sort()
    .at(0);
  if (!candidate) fail(`missing ${suffix} bundle under ${fullDir}`);
  return resolve(fullDir, candidate);
}

function run(command, args, options = {}) {
  const rendered = [command, ...args].join(" ");
  if (dryRun) return { command, args, rendered, skipped: true };
  console.log(`$ ${rendered}`);
  const result = spawnSync(command, args, {
    cwd: repoRoot,
    stdio: "inherit",
    env: process.env,
    ...options,
  });
  if (result.status !== 0) {
    fail(`${rendered} failed with status ${result.status ?? "unknown"}`);
  }
  return { command, args, rendered, skipped: false };
}

function ensureDockerImage(image) {
  const inspect = spawnSync("docker", ["image", "inspect", image], {
    cwd: repoRoot,
    stdio: dryRun ? "pipe" : "ignore",
  });
  if (inspect.status === 0) return;
  run("docker", ["pull", image]);
}

function dockerSmoke({ image, name, files, script }) {
  if (dryRun) {
    return {
      image,
      name,
      files,
      script,
      rendered: `docker create ${image} bash -lc <${name}-script>`,
      skipped: true,
    };
  }

  ensureDockerImage(image);
  const create = spawnSync("docker", ["create", image, "bash", "-lc", script], {
    cwd: repoRoot,
    encoding: "utf8",
  });
  if (create.status !== 0) {
    process.stderr.write(create.stderr);
    fail(`docker create ${image} failed`);
  }
  const containerId = create.stdout.trim();
  try {
    for (const [host, containerPath] of files) {
      run("docker", ["cp", host, `${containerId}:${containerPath}`]);
    }
    run("docker", ["start", "-a", containerId]);
  } finally {
    spawnSync("docker", ["rm", "-f", containerId], {
      cwd: repoRoot,
      stdio: "ignore",
    });
  }
  return {
    image,
    name,
    files,
    rendered: `docker create ${image} bash -lc <${name}-script>`,
    skipped: false,
  };
}

const deb = dryRun
  ? resolve(bundleRoot, "deb/discrypt_0.1.0_amd64.deb")
  : firstFile("deb", ".deb");
const rpm = dryRun
  ? resolve(bundleRoot, "rpm/discrypt-0.1.0-1.x86_64.rpm")
  : firstFile("rpm", ".rpm");
const appImage = dryRun
  ? resolve(bundleRoot, "appimage/discrypt_0.1.0_amd64.AppImage")
  : firstFile("appimage", ".AppImage");

const steps = [];
steps.push(run("dpkg-deb", ["-I", deb]));

steps.push(
  dockerSmoke({
    image: debImage,
    name: "deb-install-launch",
    files: [[deb, "/tmp/discrypt.deb"]],
    script: String.raw`
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update >/tmp/apt.log 2>&1
apt-get install -y /tmp/discrypt.deb xvfb dbus-x11 >/tmp/install.log 2>&1
command -v discrypt-desktop
rm -rf /tmp/discrypt-home
mkdir -p /tmp/discrypt-home
set +e
timeout 8s dbus-run-session -- xvfb-run -a env HOME=/tmp/discrypt-home XDG_DATA_HOME=/tmp/discrypt-home/.local/share WEBKIT_DISABLE_COMPOSITING_MODE=1 discrypt-desktop >/tmp/discrypt-smoke.log 2>&1
code=$?
set -e
tail -40 /tmp/install.log || true
tail -80 /tmp/discrypt-smoke.log || true
test "$code" -eq 0 -o "$code" -eq 124
`,
  }),
);

steps.push(
  dockerSmoke({
    image: rpmImage,
    name: "rpm-install-launch",
    files: [[rpm, "/tmp/discrypt.rpm"]],
    script: String.raw`
set -euo pipefail
rpm -qpR /tmp/discrypt.rpm >/tmp/rpm-requires.log
dnf install -y /tmp/discrypt.rpm xorg-x11-server-Xvfb dbus-x11 >/tmp/install.log 2>&1
command -v discrypt-desktop
rm -rf /tmp/discrypt-home
mkdir -p /tmp/discrypt-home
set +e
timeout 8s dbus-run-session -- bash -lc 'Xvfb :99 -screen 0 1280x720x24 >/tmp/xvfb.log 2>&1 & xvfb_pid=$!; export DISPLAY=:99 HOME=/tmp/discrypt-home XDG_DATA_HOME=/tmp/discrypt-home/.local/share WEBKIT_DISABLE_COMPOSITING_MODE=1; discrypt-desktop >/tmp/discrypt-smoke.log 2>&1; code=$?; kill "$xvfb_pid" >/dev/null 2>&1 || true; exit "$code"'
code=$?
set -e
cat /tmp/rpm-requires.log
tail -40 /tmp/install.log || true
tail -80 /tmp/xvfb.log || true
tail -80 /tmp/discrypt-smoke.log || true
test "$code" -eq 0 -o "$code" -eq 124
`,
  }),
);

steps.push(
  dockerSmoke({
    image: appImageSmokeImage,
    name: "appimage-launch",
    files: [[appImage, "/tmp/discrypt.AppImage"]],
    script: String.raw`
set -euo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update >/tmp/apt.log 2>&1
apt-get install -y xvfb dbus-x11 >/tmp/install.log 2>&1
chmod +x /tmp/discrypt.AppImage
rm -rf /tmp/discrypt-home
mkdir -p /tmp/discrypt-home
set +e
timeout 8s dbus-run-session -- xvfb-run -a env APPIMAGE_EXTRACT_AND_RUN=1 HOME=/tmp/discrypt-home XDG_DATA_HOME=/tmp/discrypt-home/.local/share WEBKIT_DISABLE_COMPOSITING_MODE=1 /tmp/discrypt.AppImage >/tmp/discrypt-smoke.log 2>&1
code=$?
set -e
tail -40 /tmp/install.log || true
tail -80 /tmp/discrypt-smoke.log || true
test "$code" -eq 0 -o "$code" -eq 124
`,
  }),
);

const plan = {
  bundleRoot,
  artifacts: { deb, rpm, appImage },
  images: { debImage, rpmImage, appImageSmokeImage },
  dryRun,
  steps,
};

if (dryRun) console.log(JSON.stringify(plan, null, 2));
else console.log("smoke-linux-packages: package install/launch smoke passed");
