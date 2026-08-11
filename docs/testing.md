# Testing

## Required Baselines

Migration work must preserve at least:

- 71 frontend tests in the private uTools integration.
- 129 passing default Rust tests and 2 explicitly ignored Windows OCR tests in the shared helper.

The verified migration baseline is currently 75 frontend tests in the private integration and 136 passing Rust tests with 2 OCR tests ignored.

The standalone desktop baseline is 63 frontend tests across 12 files, zero Svelte check errors or warnings, and 10 Tauri host tests.

## Local Gates

Every cross-host change runs the closest unit and contract tests first, followed by:

1. Svelte type checking, frontend tests, and production build.
2. Rust formatting and the full helper test suite with the pinned Windows gnullvm toolchain.
3. Tauri compilation and package build on Windows 11 x64.
4. IPC, 64-update configuration stress, gesture, window-drag, and reliability smoke tests.
5. Artifact inventory and secret scan for installers and portable output.

Tests must not be skipped, converted to TODOs, weakened, or replaced with mocks of the behavior under test merely to satisfy a gate.

## Lifecycle and Compatibility

Acceptance must exercise separate uTools and desktop data directories, intentional helper lock contention with a nonzero result, recovery after the first helper exits, schema v7 migration, unknown action preservation, token creation, normal stop, and original configuration restoration after smoke tests. The executable lock check is `node scripts/helper-instance-smoke.mjs <packaged-helper.exe>`; it requires the failure log marker `HELPER_INSTANCE_CONFLICT` before testing recovery.

After `npm run desktop:build`, run both packaged desktop lifecycle gates:

```powershell
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
```

The normal gate verifies helper readiness, schema v7 persistence, graceful stop, and zero sidecar residue. The conflict gate creates its own lock-holding helper, requires the desktop log marker `HELPER_INSTANCE_CONFLICT`, and then stops the holder through the authenticated protocol. Both gates require a fresh explicit temporary data root, place WebView data under that root, reject writes to the real application-data directory, and remove their temporary data unless `-KeepData` is requested for diagnosis.

## Installation Acceptance

After building and auditing the artifacts, run:

```powershell
npm run desktop:install-smoke
```

The gate silently installs the current-user NSIS package below a disposable directory, verifies the installed executable and complete helper payload, launches it with an isolated application/WebView data root, and then runs the real uninstaller. It requires the install directory, matching HKCU uninstall entry, and any matching current-user shortcuts to be removed. The portable package is exercised separately by the two runtime smoke commands. Artifact inspection rejects repository-private files, credentials, user configuration, logs, `node_modules`, Rust `target`, source caches, or undeclared binaries, and scans every tracked or untracked non-ignored public source file for credentials before the source is committed.

A clean clone of this repository must reproduce the desktop build. Manual Windows checks remain required for tray behavior, optional startup, global input, hot zones, gestures, drag, edge hiding, screenshots, OCR, and topmost controls; passing unit tests alone is not release evidence.
