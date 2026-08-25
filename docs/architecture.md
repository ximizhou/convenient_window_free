# Architecture

## Product Boundary

```text
Standalone desktop (Tauri 2 + Svelte) ----\
                                           > shared localhost protocol -> Rust helper
Host integration (Svelte + preload) --------/
```

This repository is authoritative for the standalone desktop and the helper. Host integrations consume it as a submodule and supply only their host-specific adapters.

## Source Layout

- `apps/desktop/`: reusable Svelte UI, typed host bridge, and Tauri 2 host.
- `helper/`: platform-independent core, IPC, storage, and platform adapters.
- `helper/src/platform/windows/`: accepted Windows implementation.
- `scripts/`: reproducible development, packaging, smoke-test, and artifact-audit entry points.

Linux and macOS modules may satisfy shared interfaces in the future. Core code must not accumulate host checks or new Linux/macOS conditional branches as a substitute for platform adapters.

## Host Bridge

Shared UI code depends on a typed bridge for lifecycle, configuration, token access, file dialogs, external links, diagnostics, and host actions. Protocol v6 emits `host.action` with generic kinds and values; the uTools adapter maps redirect actions to its host API. The helper accepts the legacy `utools-redirect` configuration value as an alias, while normalized helper state uses `host-action`. The standalone adapter hides uTools-only actions while preserving unknown action records loaded from schema v7 configuration.

## Configuration Ownership

The shared schema v7 `edgeHide.keepExpandedWhenForeground` setting defaults to `true`. An expanded edge-hidden window remains open while it is still the active foreground window; when the setting is disabled, leaving the window starts the normal restore-delay countdown even if that window remains foreground. Missing fields preserve the prior behavior, while an explicit `false` survives host normalization and helper deserialization.

The desktop host and helper never write the same file:

- The desktop host is the only writer of `<app-data>/desktop-settings.json`. This is the UI's authoritative settings snapshot.
- The helper is the only writer of `<app-data>/helper-data/config.json`. This is the last runtime configuration it accepted.

The UI awaits a successful durable desktop-settings write before sending that exact revision to the helper. A failed write is visible to the UI and is never applied to the helper. On first launch after upgrading from the early shared-file layout, the desktop host copies the legacy helper configuration into `desktop-settings.json` only when the new file does not exist; it never overwrites an existing desktop settings file.

## Helper Lifecycle

Each host starts the helper with an absolute `--data-dir` owned by that product. Authentication tokens, runtime configuration, usage data, and logs remain separate between products. The named mutex `Global\ConvenientWindowHelper` prevents both products from running helpers concurrently; the losing process logs `HELPER_INSTANCE_CONFLICT` and exits nonzero so the host can show a stable error.

The desktop package includes the helper EXE and all GNU runtime DLLs required by that exact build. The Tauri process resolves the packaged sidecar set, assigns every desktop-owned helper to a Windows Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, and retains the Job handle for the helper lifetime. Normal exits first request authenticated `helper.stop`; if the desktop process is terminated before that path runs, closing the Job handle terminates only the assigned child. Cleanup never searches for or kills helpers by executable name, so a separately owned uTools helper is outside the desktop lifecycle boundary.

The NSIS pre-uninstall hook signals `Local\com.ximizhou.convenientwindow.shutdown` and waits briefly for the desktop process to use the same guarded shutdown path before files are removed. Tray quit, Tauri exit events, and uninstall converge on a one-time shutdown guard. Closing the main window only hides it. Optional startup registration is exposed only as the checked `开机自动启动` tray item; autostart launches with `--autostart` and keeps the settings window hidden.

Automated runtime tests may set the absolute `CONVENIENT_WINDOW_DATA_DIR` override. In that mode desktop settings, helper data, and WebView data all stay below the explicit root. Production startup uses the platform application-data path.

Edge-hide state keeps the target edge and restore geometry across monitor changes. The helper renders a restore strip only when the restore rectangle still intersects the current monitor topology and the live window rectangle still matches the hidden rectangle. Empty or changed monitor snapshots, externally moved or hidden windows, and removed displays therefore cannot leave a stale pale outline; minimized windows retain their recoverable strip.

## Platform and Release Boundary

Only Windows 11 x64 is accepted. The package produces a per-user NSIS installer and a portable archive, with tray controls and optional startup registration. Public GitHub Releases use immutable final-version assets: a clean `main` build is published as a Pre-release for online acceptance, then promoted in place. Automatic updates and trusted commercial code signing are not implemented.
