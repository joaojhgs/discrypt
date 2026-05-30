# G127 static no-placeholder gate

G127 scans production-gated modules for unfinished implementation markers and
wording that would hide a placeholder production path. The gate covers Rust and
TypeScript source under `crates/*/src`, `apps/desktop/src-tauri/src`, and
`apps/ui/src`.

Forbidden production-source tokens are: `TODO`, `FIXME`, `todo!`,
`unimplemented!`, `panic!("not implemented")`, `shim`, `emulation`, `facade`,
`skeleton`, `fixture`, `local-only`, and `local only`.

Run:

```sh
npm --prefix apps/ui run test:no-placeholders-g127
```

The companion G128 gate owns explicit allowlisting for test-only or documentation
occurrences. This G127 gate is intentionally scoped to production source so CI can
fail fast before packaging or release claims are made.
