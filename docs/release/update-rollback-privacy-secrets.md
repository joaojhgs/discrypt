# Update, rollback, crash-report privacy, and secrets policy

This policy defines release operations for Discrypt desktop/mobile artifacts and
backend services. It does not enable automatic updates or crash-report upload by
itself; those runtime paths remain release-gated until signed artifact delivery,
rollback drills, and opt-in privacy controls pass their gates.

## Update channels

| Channel | Audience | Artifact source | Promotion gate |
| --- | --- | --- | --- |
| `internal` | release engineers and security reviewers | CI artifacts retained from protected branches | Full local gates plus package smoke for the target platform. |
| `beta` | opted-in testers | signed artifacts and signed update manifest | Internal channel passed, rollback drill completed, no open severity-1/2 defects. |
| `stable` | general users | signed artifacts and signed update manifest | Beta soak window completed, release review approved, reproducibility evidence archived. |

Tauri updater configuration is intentionally absent from `tauri.conf.json` today.
A future updater enablement must add a public updater signing key, signed
manifests, rollback tests, and UI copy that states the exact channel.

## Rollback policy

1. Every release candidate records git commit, lockfile hashes, package hashes,
   generated SBOM path, signing identity, and release channel.
2. Desktop packages are immutable. A rollback publishes the previous known-good
   artifact and manifest version; it does not mutate an already published file.
3. Backend services roll back by redeploying the previous container image digest
   and previous service configuration, excluding rotated secrets.
4. Rollback triggers include: startup crash, migration failure, message loss,
   media-route regression, credential leakage, unsigned artifact, failed package
   smoke, or privacy policy violation.
5. Rollback verification repeats install/launch smoke, signaling `/healthz`,
   `/metrics`, and an opaque signal exchange before promotion is restored.

## Crash-report privacy policy

Crash reporting is opt-in only. Until the app implements opt-in controls and a
redaction pipeline, crash upload is off by default and no release may claim
remote crash reporting.

Allowed crash data after opt-in:

- application version, target OS, CPU architecture, package channel;
- crash timestamp rounded to minute precision;
- redacted stack frames and module offsets;
- coarse feature area selected from an allowlist (`startup`, `storage`,
  `signaling`, `text`, `voice`, `update`, `unknown`).

Forbidden crash data:

- message bodies, attachment bytes, media frames, SDP, ICE credentials, STUN/TURN
  credentials, MLS secrets, SFrame keys, recovery codes, invite secrets, room
  names, usernames, device names, profile display names, endpoint tokens, and raw
  database rows;
- full filesystem paths or environment variables;
- network packet captures or logs that include app payload bytes.

Crash uploads must include a local preview/export path before submission, a
one-click disable control, and retention capped at 30 days unless a user exports
an incident bundle manually.

## Secrets management

`deploy/release/secrets-inventory.json` is the machine-readable source of truth
for release and service secrets. Release tooling must reject any secret that is
stored in source control, printed by `--print-config`, or included in invite
metadata when only derived short-lived credentials are required.

Minimum controls:

- `TAURI_PRIVATE_KEY` exists only in the protected updater-signing environment or offline signer;
- `EXTERNAL_TURN_STATIC_AUTH_SECRET` exists only in the TURN credential issuer and TURN service secret store;
- protected release environments for signing and notarization credentials;
- two-person review before changing signing, updater, TURN, or crash collector
  secrets;
- scheduled rotation every 90 days or sooner on membership change or suspected
  disclosure;
- generated public fingerprints/hashes archived with each release;
- no client bundle contains a private signing key, TURN static auth secret,
  signaling admin token, or crash upload collector token.

## Evidence checklist

Before promoting a release, archive:

- package hashes, SBOMs, lockfile hashes, and git commit;
- signing/notarization logs for platforms where signing is enabled;
- update manifest signature verification output when updater is enabled;
- rollback drill transcript for the channel;
- crash-report redaction test output when crash upload is enabled;
- secrets inventory review signoff and rotation status.
