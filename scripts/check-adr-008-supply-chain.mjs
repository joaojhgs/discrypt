#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path) => readFileSync(resolve(repoRoot, path), 'utf8');
const files = {
  adr: read('docs/adr/adr-008-supply-chain.md'),
  deny: read('deny.toml'),
  ci: read('.github/workflows/ci.yml'),
  cargoToml: read('Cargo.toml'),
  cargoLock: read('Cargo.lock'),
  packageLock: read('apps/ui/package-lock.json'),
  packageJson: read('apps/ui/package.json'),
  releasePolicy: read('docs/release/update-rollback-privacy-secrets.md'),
};

const failures = [];
function requireText(name, token) {
  if (!files[name].includes(token)) failures.push(`${name} missing token: ${token}`);
}
function run(label, cmd, args) {
  const result = spawnSync(cmd, args, { cwd: repoRoot, encoding: 'utf8' });
  if (result.status !== 0) failures.push(`${label} failed:\n${result.stdout}\n${result.stderr}`.trim());
}

for (const token of [
  '# ADR-008: Supply chain, SBOM, licenses, and reproducibility',
  'cargo audit',
  'cargo deny check',
  'npm audit --audit-level=high --omit=dev',
  'cargo sbom --output-format spdx_json_2_3',
  'Crypto-sensitive dependency policy',
  'License policy',
  'Reproducible build assumptions',
  'CI artifact storage',
  'release remains blocked',
  'owner, reason, expiry, and\nupgrade path',
]) requireText('adr', token);

for (const token of [
  'yanked = "deny"',
  'wildcards = "deny"',
  'unknown-registry = "deny"',
  'unknown-git = "deny"',
  'MIT',
  'Apache-2.0',
  'AGPL-3.0-or-later',
  'MPL-2.0',
  'ISC',
]) requireText('deny', token);

for (const token of [
  'supply-chain',
  'cargo install cargo-audit --locked',
  'cargo install cargo-deny --locked',
  'cargo install cargo-sbom --locked',
  'cargo audit',
  'cargo deny check',
  'cargo sbom --output-format spdx_json_2_3 > discrypt.spdx.json',
  'npm audit --audit-level=high --omit=dev',
  'actions/upload-artifact@v4',
  'discrypt-sbom',
]) requireText('ci', token);

for (const token of ['license = "AGPL-3.0-or-later"', '[workspace.dependencies]']) requireText('cargoToml', token);
for (const token of ['openmls', 'webrtc', 'aes-gcm']) requireText('cargoLock', token);
for (const token of ['"lockfileVersion"', '"packages"']) requireText('packageLock', token);
for (const token of ['test:adr-008-supply-chain']) requireText('packageJson', token);
for (const token of ['SBOMs, lockfile hashes, and git commit', 'reproducibility evidence archived']) requireText('releasePolicy', token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(files.adr)) failures.push('ADR-008 contains unfinished-work marker');
if (/ignore = \[[^\]]+\]/.test(files.deny)) failures.push('deny.toml advisory ignore list must remain empty for G109 policy lock');

run('cargo metadata locked', 'cargo', ['metadata', '--locked', '--format-version', '1', '--no-deps']);
run('cargo deny licenses', 'cargo', ['deny', 'check', 'licenses', '--hide-inclusion-graph']);
run('cargo deny bans sources', 'cargo', ['deny', 'check', 'bans', 'sources', '--hide-inclusion-graph']);
run('npm production audit', 'npm', ['--prefix', 'apps/ui', 'audit', '--audit-level=high', '--omit=dev']);

if (failures.length > 0) {
  console.error('ADR-008 supply-chain check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log('ADR-008 supply-chain check passed.');
