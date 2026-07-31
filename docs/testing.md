# Testing

## Required Baselines

Migration work must preserve at least:

- 71 frontend tests in the private uTools integration.
- 129 passing default Rust tests and 2 explicitly ignored Windows OCR tests in the shared helper.

The verified migration baseline is currently 75 frontend tests in the private integration and 136 passing Rust tests with 2 OCR tests ignored.

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

## Installation Acceptance

Use an isolated user/data directory to install, launch, exercise, exit, and uninstall the per-user NSIS package. Run the portable package separately. Inspect package contents and installed files to reject repository-private files, credentials, user configuration, logs, `node_modules`, Rust `target`, source caches, or undeclared binaries.

A clean clone of this repository must reproduce the desktop build. Manual Windows checks remain required for tray behavior, optional startup, global input, hot zones, gestures, drag, edge hiding, screenshots, OCR, and topmost controls; passing unit tests alone is not release evidence.
