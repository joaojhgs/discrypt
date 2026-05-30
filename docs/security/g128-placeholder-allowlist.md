# G128 placeholder allowlist gate

G128 makes the G127 review-pattern scan auditable. Every remaining occurrence of
placeholder-sensitive wording in production-gated source, Tauri source, UI source,
or CI workflow files must be present in `g128-placeholder-allowlist.json` with:

- `path`: exact repository path;
- `pattern`: reviewed token class (`shim`, `emulat`, `facade`, `skeleton`,
  `fixture`, `local-only`, `local dev`, `local-dev`, or `mock`);
- `expected`: exact trimmed source line; and
- `reason`: release-review rationale proving the occurrence is test-only,
  documentation/honesty metadata, a real verification environment, or a
  compile-time non-production feature gate.

Run:

```sh
npm --prefix apps/ui run test:placeholder-allowlist-g128
```

The checker fails on unallowlisted new occurrences and stale allowlist entries.
G129 is responsible for the stricter release-build check that production command
paths and UI copy do not depend on local development fallbacks.
