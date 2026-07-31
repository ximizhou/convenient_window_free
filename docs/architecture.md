# Architecture

## Product Boundary

```text
Standalone desktop (Tauri 2 + Svelte) ----\
                                           > shared localhost protocol -> Rust helper
Private uTools host (Svelte + preload) ----/
```

This repository is authoritative for the standalone desktop and the helper. The private integration consumes it as a submodule and supplies only its uTools-specific host adapter.

## Source Layout

- `apps/desktop/`: reusable Svelte UI, typed host bridge, and Tauri 2 host.
- `helper/`: platform-independent core, IPC, storage, and platform adapters.
- `helper/src/platform/windows/`: accepted Windows implementation.
- `scripts/`: reproducible development, packaging, smoke-test, and artifact-audit entry points.

Linux and macOS modules may satisfy shared interfaces in the future. Core code must not accumulate host checks or new Linux/macOS conditional branches as a substitute for platform adapters.

## Host Bridge

Shared UI code depends on a typed bridge for lifecycle, configuration, token access, file dialogs, external links, diagnostics, and host actions. Protocol v6 emits `host.action` with generic kinds and values; the uTools adapter maps redirect actions to its host API. The helper accepts the legacy `utools-redirect` configuration value as an alias, while normalized helper state uses `host-action`. The standalone adapter hides uTools-only actions while preserving unknown action records loaded from schema v7 configuration.

## Helper Lifecycle

Each host starts the helper with an absolute `--data-dir` owned by that product. Authentication tokens, configuration, usage data, and logs remain separate. The named mutex `Global\ConvenientWindowHelper` prevents both products from running helpers concurrently; the losing process logs `HELPER_INSTANCE_CONFLICT` and exits nonzero so the host can show a stable error.

The desktop package includes the helper EXE and all GNU runtime DLLs required by that exact build. The Tauri process resolves the packaged sidecar set, starts and stops it cleanly, and surfaces startup, protocol, and recovery diagnostics.

## Platform and Release Boundary

Only Windows 11 x64 is accepted. The first package produces a per-user NSIS installer and a portable archive or executable, with tray controls and optional startup registration. Automatic updates, code signing, public GitHub Releases, and release tags are outside the initial implementation gate.
