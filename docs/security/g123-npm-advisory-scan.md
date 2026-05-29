# G123 npm advisory scan

## Scope

G123 gates npm advisory status for the UI dependency graph under
`apps/ui/package-lock.json`. The gate runs against the committed lockfile and
covers both production runtime dependencies and the full UI dependency graph used
by build/test tooling.

## Required commands

- `npm --prefix apps/ui run test:npm-audit-g123`
- `npm --prefix apps/ui audit --audit-level=high --omit=dev`
- `npm --prefix apps/ui audit --audit-level=high`

## Current result

Both commands pass with zero high-or-critical advisories. There are no G123 npm
advisory waivers.

## Release rule

A production release candidate must not ship with high-or-critical advisories in
UI production dependencies. Build/test-only high-or-critical advisories also
block release automation unless a documented non-release waiver names the npm
package, advisory URL/id, dependency path, owner, reason, expiry, mitigation, and
upgrade path.
