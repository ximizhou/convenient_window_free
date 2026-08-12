# Agent Instructions

## Read First

1. Read `README.md`, then route by the path being changed.
2. Desktop UI or Tauri host work: read `docs/architecture.md`, `docs/testing.md`, and files under `apps/desktop/`.
3. Shared helper/core work: read `docs/architecture.md`, `docs/testing.md`, and files under `helper/`.
4. Cross-host protocol or configuration work: inspect both `apps/desktop/` and `helper/`, including their contract tests.
5. If `PROGRESS.md` or `BLOCKED.md` exists in the enclosing integration workspace, read it before resuming a long task.

## Repository Boundary

- This repository is the source of truth for the standalone desktop app and the single shared Rust helper source tree.
- The private uTools integration consumes this repository as an `open-source/` submodule. Do not create or maintain a second helper source tree there.
- Keep host-specific APIs behind typed host bridges. Shared Svelte components must not call uTools or Tauri globals directly.
- Keep helper Windows platform implementations in `helper/src/platform/windows/`. Windows-only Tauri host lifecycle code may live under `apps/desktop/src-tauri/` when it owns desktop process, tray, installer, or sidecar behavior. Preserve platform interfaces for future Linux/macOS work, but do not claim those platforms are supported.
- Windows 11 x64 is the only supported and accepted target for the first release.

## Engineering Rules

- Preserve schema v7 migrations and unknown action records when reading and writing existing configuration.
- Keep desktop settings and helper runtime configuration in separate single-writer files; await durable host persistence before sending that revision to the helper.
- The uTools and desktop products use separate data directories and must not run their helpers concurrently. A conflict must return a nonzero process result and a host-readable error.
- Package the helper executable with every required runtime DLL. A bare EXE is not a valid desktop sidecar bundle.
- The NSIS installer and portable package must include the repository-root `LICENSE`; artifact inventory checks must fail when it is missing.
- Do not weaken, skip, or delete tests to make a change pass. Add focused tests for protocol, migration, process, and packaging changes.
- Do not create a GitHub Release, tag, or upload release assets without explicit approval. Source commits and branch pushes are allowed after verification.
- Do not commit `node_modules/`, Rust `target/`, local configuration, tokens, logs, generated installers, or private-repository material.
