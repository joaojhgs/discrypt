# Discrypt local patch

This directory vendors the source of crates.io release `libopus-rs 0.0.1` because that release was yanked without a published replacement. The source remains required by Discrypt's pure-Rust Opus media path.

Patch scope:

- crate version suffix: `0.0.1+discrypt.1`
- upstream Rust source: unchanged
- package metadata: reduced to the library targets used by Discrypt

Release rule: replace this local patch with a non-yanked upstream release as soon as one exists and passes the media, voice, workspace, audit, and deny gates.
