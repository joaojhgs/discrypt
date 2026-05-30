# G129 release no-fallback gate

G129 makes release packaging fail before build output is produced when production
commands or UI copy would depend on local development adapters.

The gate enforces three conditions:

1. `scripts/release-linux.mjs` rejects `DISCRYPT_RELEASE_FEATURES` containing
   `harness` or `local-dev`.
2. The Linux release plan runs `npm --prefix apps/ui run
   test:release-no-fallback-g129` before UI build, Rust desktop tests, Tauri
   packaging, SBOM generation, or reproducibility evidence.
3. The UI fallback client remains usable only in `import.meta.env.DEV` or when
   `VITE_DISCRYPT_LOCAL_DEV_FALLBACK=1` is explicitly set for local tests; the
   main production UI must not embed fallback state/copy.

Run:

```sh
npm --prefix apps/ui run test:release-no-fallback-g129
npm --prefix apps/ui run test:release-linux
```
