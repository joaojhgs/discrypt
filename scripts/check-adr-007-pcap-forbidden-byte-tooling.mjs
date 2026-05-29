#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const read = (path) => readFileSync(resolve(repoRoot, path), 'utf8');
const files = {
  adr: read('docs/adr/adr-007-pcap-and-forbidden-byte-tooling.md'),
  g096: read('docs/security/g096-pcap-acceptance-suite.md'),
  signaling: read('external/signaling-repository/src/lib.rs'),
  webrtcPaths: read('external/signaling-repository/tests/process_webrtc_transport_paths.rs'),
  signalExchange: read('external/signaling-repository/tests/process_signal_exchange.rs'),
  harness: read('harness/multinode/src/lib.rs'),
  pcapCheck: read('scripts/check-pcap-suite-g096.mjs'),
  packageJson: read('apps/ui/package.json'),
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
  '# ADR-007: Pcap and forbidden-byte tooling',
  'capture tooling',
  'Forbidden-token generation',
  'Redaction rules',
  'CI artifact storage',
  'Pass/fail thresholds',
  'AuditFixture',
  'PcapEvent',
  'MetadataMatrix::approved_v1',
  'process_webrtc_transport_paths',
  'process_signal_exchange',
  'pcap_acceptance_matrix_smoke',
  'pcap_forbidden_byte_tooling_decision',
  'covers_adr_007',
  'external libpcap/tcpdump',
  'zero forbidden-byte matches',
  'forbidden_tokens_scanned',
]) requireText('adr', token);

for (const token of [
  'PcapForbiddenByteToolingDecision',
  'pcap_forbidden_byte_tooling_decision',
  'covers_adr_007',
  'AuditFixture',
  'PcapEvent',
  'MetadataMatrix',
  'contains_any_token',
  'zero forbidden-byte matches',
  'redacted pcap matrix',
]) requireText('signaling', token);

for (const token of [
  'Forbidden-byte sentinel classes',
  'does not claim external libpcap/tcpdump capture',
  'pcap_acceptance_matrix_covers_ac1_ac8_ac15_ac18_and_metadata',
]) requireText('g096', token);

for (const token of ['two_process_webrtc_paths_pass_with_ciphertext_only_pcap_audit', 'direct', 'overlay', 'turn', 'no_forbidden_content_egress', 'matches_matrix']) requireText('webrtcPaths', token);
for (const token of ['separate_process_clients_exchange_generated_offer_answer_and_candidate', 'forbidden_tokens_scanned', 'assert_no_forbidden_plaintext', 'zero_linkage']) requireText('signalExchange', token);
for (const token of ['PcapAcceptanceMatrixSmoke', 'forbidden_scanner_covers_release_tokens', 'MetadataMatrix::approved_v1', 'contains_any_token(b"prefix message-body suffix"', 'contains_any_token(b"prefix mls-epoch-secret suffix"']) requireText('harness', token);
for (const token of ['test:pcap-suite-g096', 'test:adr-007-pcap-forbidden-byte-tooling']) requireText('packageJson', token);
for (const token of ['G096 pcap suite check passed', 'external-signaling', 'process_signal_exchange']) requireText('pcapCheck', token);

if (/TODO|FIXME|unimplemented!|todo!/i.test(files.adr)) failures.push('ADR-007 contains unfinished-work marker');
if (/external host packet captures pass|libpcap capture passed|tcpdump capture passed/i.test(files.adr)) failures.push('ADR-007 overclaims external capture evidence');
if (/raw packet captures.*normal CI artifacts/i.test(files.adr) === false) failures.push('ADR-007 must forbid raw packet captures in normal CI artifacts');

run('ADR-007 decision unit', 'cargo', ['test', '-p', 'external-signaling', 'pcap_forbidden_byte_tooling_decision_covers_adr_007', '--quiet']);
run('G096 pcap suite', 'npm', ['--prefix', 'apps/ui', 'run', 'test:pcap-suite-g096']);

if (failures.length > 0) {
  console.error('ADR-007 pcap/forbidden-byte tooling check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}
console.log('ADR-007 pcap/forbidden-byte tooling check passed.');
