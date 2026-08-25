# Testing

## Required Baselines

Migration work must preserve at least:

- 71 frontend tests in the host integration.
- 129 passing default Rust tests and 2 explicitly ignored Windows OCR tests in the shared helper.

The current verified baseline is 97 frontend tests in the host integration and 142 passing Rust tests with 2 OCR tests ignored.

The standalone desktop baseline is 74 frontend tests across 13 files, zero Svelte check errors or warnings, and 13 Tauri host tests.

## Local Gates

Every cross-host change runs the closest unit and contract tests first, followed by:

1. Svelte type checking, frontend tests, and production build.
2. Rust formatting and the full helper test suite with the pinned Windows gnullvm toolchain.
3. Tauri compilation and package build on Windows 11 x64.
4. IPC, 64-update configuration stress, gesture, window-drag, and reliability smoke tests.
5. Artifact inventory and secret scan for installers and portable output.

Tests must not be skipped, converted to TODOs, weakened, or replaced with mocks of the behavior under test merely to satisfy a gate.

Cross-host UI acceptance requires settings to persist on each valid change without a generic manual-save button. The hot-zone master switch must leave saved configuration, preview, and configured markers visible while the editing controls are inert. Window enhancement currently exposes only the edge-hide tutorial: one circled question mark beside its heading, a hover/focus card that remains readable while the pointer enters it, the center-to-right/collapse/restore CSS sequence, no drag or pin tutorial, no horizontal overflow at 1280x720, 900x600, or 640x600, and no positional animation under reduced motion.

The capture-exclusion assertions remain strict on the supported Windows 11 workstation target. Windows Server CI may report display affinity `0` after a successful `SetWindowDisplayAffinity` call; tests recognize that product type explicitly rather than weakening the Windows 11 assertion.

## Lifecycle and Compatibility

Acceptance must exercise separate uTools and desktop data directories, intentional helper lock contention with a nonzero result, recovery after the first helper exits, schema v7 migration, unknown action preservation, token creation, normal stop, and original configuration restoration after smoke tests. Edge-hide coverage must verify that `keepExpandedWhenForeground` defaults on without changing existing configurations, preserves an explicit `false`, keeps an expanded foreground window open when enabled, and allows the same window to recollapse after the restore delay when disabled. While checking the blue edge preview, a stationary left-button press on a window already touching an outer edge must not show a preview; the line may appear only after pointer motion during a real drag and must disappear after release, including on a left/negative-coordinate monitor. Restore-strip coverage must clear the pale outline when the monitor snapshot is empty, the original monitor is removed, or the collapsed window is externally moved or hidden, while retaining it for a minimized window. The executable lock check is `node scripts/helper-instance-smoke.mjs <packaged-helper.exe>`; it requires the failure log marker `HELPER_INSTANCE_CONFLICT` before testing recovery.

After `npm run desktop:build`, run all packaged desktop lifecycle gates:

```powershell
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
npm run desktop:runtime-force-kill-smoke
```

The normal gate verifies helper readiness, schema v7 persistence, graceful stop, and zero sidecar residue. The conflict gate creates its own lock-holding helper, requires the desktop log marker `HELPER_INSTANCE_CONFLICT`, and then stops the holder through the authenticated protocol. The force-kill gate waits for the real packaged helper, force-terminates the owning desktop process, and requires the Job Object to remove the assigned sidecar and close port `56873`. All gates require a fresh explicit temporary data root, place WebView data under that root, reject writes to the real application-data directory, and remove their temporary data unless `-KeepData` is requested for diagnosis.

## Installation Acceptance

After building and auditing the artifacts, run:

```powershell
npm run desktop:install-smoke
```

The gate silently installs the current-user NSIS package below a disposable directory, verifies the installed executable, repository-root `LICENSE`, and complete helper payload, and launches it with isolated application/WebView data. It invokes the real uninstaller while the desktop app and its helper are still running, requiring the named shutdown event to produce a graceful helper log, exit both processes, and close port `56873` before the install directory is removed. It then reinstalls, starts a helper from a separate uTools-owned payload/data path, confirms the desktop reports `HELPER_INSTANCE_CONFLICT`, uninstalls the desktop, and requires that external helper and its port to remain alive until the test stops it through authenticated IPC. Both passes require the matching HKCU uninstall entry, current-user shortcuts, install directory, and test processes to be removed. The portable package is exercised separately by the three runtime smoke commands. Artifact inspection requires the repository-root `LICENSE` and generated `THIRD-PARTY-NOTICES.txt` in the portable directory, portable ZIP, and NSIS payload; it verifies representative npm/Cargo components and MIT, Apache, BSD, and MPL terms, rejects the project PolyForm text inside third-party notices, rejects repository-private files, credentials, user configuration, logs, `node_modules`, Rust `target`, source caches, or undeclared binaries, and scans every tracked or untracked non-ignored public source file for credentials before the source is committed.

The install gate first runs `scripts/read-text-file-with-retry.test.ps1`, which holds a synthetic helper log with `FileShare.None` and requires the shared reader to recover after the lock is released. Runtime and install scripts retry only transient `IOException` reads within a bounded deadline; readiness markers, process identity, port closure, uninstall cleanup, and helper-ownership assertions remain strict.

A clean clone of this repository must reproduce the desktop build. Manual Windows checks remain required for tray behavior, optional startup, global input, hot zones, gestures, drag, edge hiding, screenshots, OCR, and topmost controls; passing unit tests alone is not release evidence.

There are only two user acceptance phases. Daily `develop` acceptance normally uses the host integration; build and hand off a local NSIS package only when the user explicitly requests desktop synchronization. Before release, rebuild from clean `main`, publish the exact final installer and portable archive as a GitHub Pre-release, and test the public downloads end to end. The final manifest must identify the clean `main` commit and `SHA256SUMS` must match both deliverables. Stable promotion is allowed only for the same remote assets; changed binaries require a new patch version and tag.
