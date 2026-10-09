# Testing

## Startup and scroll-layout follow-up (0.6.4, 2026-10-10)

The desktop frontend passes 25 test files / 272 tests, zero-error/warning Svelte checks and a production frontend build. A compiled geometry DOM regression keeps editor nodes mounted across helper readiness changes while still cancelling drafts at display/area boundaries. Production UI smoke covers 800×544, 359×544, 640×600 and 800×600 layouts with ten power/hint cycles per size, failed-save and duplicate-start protection, unchanged-display snapshot reuse and reachable bottom content. The browser uses mocked host/IPC and does not replace native Windows, DPI or multi-monitor acceptance.

Verify the master switch reports progress promptly, does not start after a failed save, and preserves action/hint choices. Scroll to geometry and the last settings section before and after disconnect/reconnect; inputs, presets and bottom content must remain reachable without clipping. Unchanged runtime display snapshots must not rebuild the preview; bounds, work-area, primary-display and identity changes must still refresh it. This frontend-only follow-up does not change the helper, schema or protocol, and previously packaged installers do not contain it until rebuilt.

## Per-zone geometry and schema-v9 acceptance

Require the exact protocol7/schema9 ready handshake before sending configuration; test old, missing and future schemas without writes or dropped geometry. Migrate earlier settings without materializing overrides or changing legacy global sizes and actions. Verify shared rectangle/normalization fixtures, corner priority over full-length edges, per-display isolation, deep copies, persistence, export/import and per-zone reset. UI regression must mount the real App, cancel numeric drafts on context changes, preserve non-square ratios, update both previews, and allow selecting all eight zones inside a narrow drawer. Browser host/IPC mocks are not native acceptance evidence.

Helper verification: 235 passed / 3 existing ignored tests, including the shared hot-zone-frame path, schema-v8 migration and stale-schema rejection.

Development evidence: the desktop frontend passes 267 tests across 25 files, with zero Svelte errors/warnings. The corresponding other-host frontend passes 327 tests across 26 files. Shared fixtures and modules pass parity checks. Real-browser acceptance covers 28 paired-host geometry scenarios and the standard modifier/language/layout smoke; host and IPC mocks are explicitly not native evidence.

Manual Windows acceptance must include bottom-edge wheel controls at centered and 100% lengths, endpoints versus corner priority, hint-off actions, independent displays, negative coordinates, portrait orientation, and 100–200% DPI. Unlinked 12×24 then linked width24 must produce 24×48. Reset removes only the current override and falls back to the legacy global thickness/40%. Do not install over existing user software automatically. New schema settings require the matching helper and are not a downgrade artifact.

The engine resolves one per-display hot-zone frame per tick for both hint and action paths; geometry adds no worker, polling or runtime I/O. Real-user responsiveness remains a manual acceptance item, not a conclusion from unit-test durations.

## Hot-zone hover hint (0.6.4)

Both frontends default the optional `showHotzoneHint` flag on for missing or malformed values and preserve false across normalization, storage, import/export, remounting, and feature-switch changes. The shared configuration fixture includes false. DOM checks mount the real App with mocked host/IPC; real-browser checks cover light/dark, English, narrow layouts, toggling and reloads. Native helper tests cover persistence, independent wheel detection, per-display actions, hiding/reusing the hot-zone HWND, and preserving the separate edge-preview hint.

Manual Windows acceptance: restart the updated helper, keep a bottom-edge volume-wheel action configured, and disable the hover hint in Corner parameters. Verify no pale background appears while scrolling still changes volume. Reenable to restore the hint, then disable and restart to verify persistence. Edge-hide hints, gesture trails, topmost pins, and the settings preview must remain unchanged. The option introduces no worker, timer, polling or runtime file reads; automated evidence is not a real-user responsiveness measurement. Custom zone lengths are not part of this change.

## Required Baselines

Historical regression floors remain useful, but they are not current candidate results. Migration work originally required 71 host-integration frontend tests and 129 default helper tests (2 Windows OCR tests explicitly ignored). The archived 0.5.9 candidate reported 107 host-integration tests, 79 standalone frontend tests, 165 helper tests (2 ignored), and 13 Tauri host tests.

The 0.6.4 source integrates PRs #21–#25 on the full 0.6.3 baseline. Record the actual candidate commit, complete automated results, ignored-test reasons, and artifact identity after the final integration run; no current totals or real-machine acceptance are asserted here. Previous release results cannot substitute for new UAC, multi-monitor/DPI, taskbar recovery, or host-adapter runtime evidence.

## Local Gates

Every cross-host change runs the closest unit and contract tests first, followed by:

1. Svelte type checking, frontend tests, and production build.
2. Rust formatting and the full helper test suite with the pinned Windows gnullvm toolchain.
3. Tauri compilation and package build on Windows 11 x64.
4. Native macOS and Linux X11 helper/Tauri compilation on their own runners; Linux uses Xvfb for display-backed checks.
5. IPC, 64-update configuration stress, gesture, window-drag, and reliability smoke tests.
6. Artifact inventory and secret scan for installers and portable output.

Tests must not be skipped, converted to TODOs, weakened, or replaced with mocks of the behavior under test merely to satisfy a gate.
Window-drag acceptance must include a real application in `windowDrag.pausedApps`: the configured modifier+mouse combination must remain usable by that application, including both the original press and release, while the same combination still moves/resizes an unlisted ordinary window. A maximized unlisted window must still restore and drag, and the low-level admission check must not change its geometry before the engine accepts the capture.


Cross-host UI acceptance requires settings to persist on each valid change without a generic manual-save button. The hot-zone master switch must leave saved configuration, preview, and configured markers visible while the editing controls are inert. Window enhancement currently exposes only the edge-hide tutorial: one circled question mark beside its heading, a hover/focus card that remains readable while the pointer enters it, the center-to-right/collapse/restore CSS sequence, no drag or pin tutorial, no horizontal overflow at 1280x720, 900x600, or 640x600, and no positional animation under reduced motion. Both hosts expose `showRestoreHint`, default it on for missing legacy fields, preserve an explicit off value, and explain that disabling it hides only the pale outline while edge restore remains active.

The capture-exclusion assertions remain strict on the supported Windows 11 workstation target. Windows Server CI may report display affinity `0` after a successful `SetWindowDisplayAffinity` call; tests recognize that product type explicitly rather than weakening the Windows 11 assertion. macOS capture tests require Screen Recording permission, and Accessibility permission is required for global input/window control; a permission denial must be surfaced as unavailable, never counted as a passing capability. Linux helper tests run under X11 (`DISPLAY` and `XDG_SESSION_TYPE=x11`); Wayland tests verify detection and explicit degradation only.

## Native Runner Gates

The repository pins a Windows GNU target in `rust-toolchain` and `helper/.cargo/config.toml`, so native jobs invoke `cargo +stable` and pass their host target explicitly. Cross-compiling Linux X11 from Windows is not a substitute for a native runner because `rdev` links to system X11 libraries through `pkg-config`.

The `macOS and Linux X11` workflow runs the desktop frontend checks, helper format/tests, and Tauri host tests/checks on macOS x64, macOS arm64, and Ubuntu x64. Ubuntu installs the X11/Tauri development libraries, starts Xvfb with `-noreset` to avoid display-reset races during lifecycle smoke, then runs `scripts/helper-instance-smoke.mjs` against the native helper. The smoke requires helper readiness, intentional single-instance conflict with a nonzero exit and `HELPER_INSTANCE_CONFLICT`, authenticated stop, and clean recovery.

Native acceptance still needs one real macOS machine with Accessibility and Screen Recording enabled, plus one Linux X11 desktop (not only Xvfb), to verify hot zones, global gestures, move/resize, foreground selection, topmost, and screenshot output. Linux Wayland is intentionally limited to the capability/degradation contract.

## Brightness Controls

Automated tests cover target-display selection, brightness range/clamping, per-display queue coalescing, duplicate/disconnected DRM connectors, backlight association, DDC packets, Windows WMI/DDC fallback resolution, GDI gamma scaling/clamping, command failure/timeout cleanup, and settings round trips. macOS protocol and value tests also run on Windows and Linux; Intel IOKit ABI layout assertions run when compiling for that target.

Hardware acceptance must verify internal/external pairs, same-model displays, negative desktop coordinates, rapid scrolling, unplug/replug, mirrored outputs, missing dependencies, denied device access, brightness limits, coexistence with desktop brightness controls, and a Windows display with WMI/DDC/CI unavailable to confirm the 70% software fallback and normal-shutdown gamma restoration. Only the triggering display should change, and mouse input must remain responsive while hardware calls are pending. Use the [platform requirements](architecture.md#brightness-controls) to prepare each machine.

WSL/Xvfb checks do not exercise physical backlight or DDC devices. macOS cross-compilation does not exercise native linking or DisplayServices. Actual display control requires hardware acceptance on each platform.

## Volume and Adjustment Feedback

The adjustment tests cover direction-preserving coalescing, display selection, PulseAudio channel ordering, readback delivery during continuous input, interaction switches and idle resumption, delayed pending visibility, indicator expiry, and frontend snapshot ordering across reconnects. Run the helper suite, desktop frontend tests/check/build, and Tauri host tests after changing the result contract.

Two native audio tests are opt-in:

- Windows: `cargo test --manifest-path helper/Cargo.toml real_endpoint_roundtrip_restores_the_original_state -- --ignored --test-threads=1`. It changes the real default endpoint and restores its original volume and mute state with a scope guard.
- Linux: `cargo +stable test --manifest-path helper/Cargo.toml real_pulse_volume_preserves_balance_and_reports_mute_and_limits -- --ignored --test-threads=1`. Point `PULSE_SERVER` at an isolated PulseAudio server with a stereo null sink named `cw_adjustment_test` as its default. The test checks channel balance, mute behavior, clamping, and movement away from zero. It leaves that disposable sink at 2%.

Indicator hardware acceptance covers successful readback, mute, unavailable devices, rapid direction changes, audio changes during slow DDC operations, helper restart, mixed display scales, fullscreen windows, and operation with settings hidden. The indicator must appear on the trigger display, preserve focus, pass pointer events through, and disappear after its deadline. Run these checks on macOS and Linux X11 as well as Windows.

## Lifecycle and Compatibility

Acceptance must exercise separate uTools and desktop data directories, intentional helper lock contention with a nonzero result, recovery after the first helper exits, schema v8 migration, unknown action preservation, token creation, normal stop, and original configuration restoration after smoke tests. Edge-hide coverage must verify that `keepExpandedWhenForeground` defaults off for new configurations without changing existing configurations, preserves an explicit `false`, keeps an expanded foreground window open when enabled, and allows the same window to recollapse after the restore delay when disabled. It must also verify the animated collapse/restore path has no visible jump, keeps the window size stable during movement, does not recollapse while the pointer is inside the restored window, and ignores transient/full-screen screenshot surfaces until a normal foreground window returns. While checking the blue edge preview, a stationary left-button press on a window already touching an outer edge must not show a preview; the line may appear only after pointer motion during a real drag and must disappear after release, including on a left/negative-coordinate monitor. Restore-strip coverage must clear both the pale outline and its pointer hotzone when the monitor snapshot is empty, a platform query fails, the original monitor is removed, the collapsed window is externally moved, hidden, or minimized, or the engine is disabled or stopped. Turning `showRestoreHint` off must clear only the outline while the same edge hotzone still restores the window. Initial and repeated collapses must not expose a strip or hotzone until live geometry confirms the hidden rectangle; a mismatch or failed collapse must retain a hint-free cleanup state, retry a failed cleanup after backoff, and restore the original topmost state before removal. After a monitor change, including adding a display outside the old edge, the old edge must remain clear while the helper relocates the live window or adopts an expected Windows relocation; the new strip and hotzone become eligible only after live and cached collapsed geometry agree. A rejected or unverified relocation must back off without blocking another collapsed window's hotzone. The executable lock check is `node scripts/helper-instance-smoke.mjs <packaged-helper.exe>`; it requires the failure log marker `HELPER_INSTANCE_CONFLICT` before testing recovery.

After `npm run desktop:build`, run all packaged desktop lifecycle gates:

```powershell
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
npm run desktop:runtime-force-kill-smoke
```

The normal gate verifies helper readiness, schema v8 persistence, graceful stop, and zero sidecar residue. The conflict gate creates its own lock-holding helper, requires the desktop log marker `HELPER_INSTANCE_CONFLICT`, and then stops the holder through the authenticated protocol. The force-kill gate waits for the real packaged helper, force-terminates the owning desktop process, and requires the Job Object to remove the assigned sidecar and close port `56873`. All gates require a fresh explicit temporary data root, place WebView data under that root, reject writes to the real application-data directory, and remove their temporary data unless `-KeepData` is requested for diagnosis.

## Experimental Taskbar Appearance Acceptance

Automatic gates cover both frontends, legacy/default-off configuration, enabled-only prototype migration, all material/color/opacity/border round-trips, material-capability discovery, native snapshot/options ABI, error/restoring states, a disabled worker, and loading the embedded DLL with its disabled export. `helper/native/tests/run-native-test.ps1` drives the production hook against isolated Win32 windows and real v2 mappings: compatible current/legacy resident reuse, unloaded stale marker cleanup, live-owner conflict and incompatible-wire refusal. It includes the reported `0x80070666` regression and never injects into Explorer. Pass `-ComponentPath <already-built-taskbar-appearance.dll>` to reuse a verified build instead of recompiling the component. Rust regressions cover retry deadlines, bounded exhaustion, cancellation and environmental conflict probes. These tests do not inject into Explorer. Frontend mock-IPC preview/apply/restore tests verify direct activation without confirmation dialogs, coherent parameter persistence, restoration and refusal to trust a material-unaware helper. They must not be presented as proof of actual taskbar rendering.

Manual acceptance must record Windows build/revision, theme, DPI, and monitor layout. Explicitly opt in, verify that only the background disappears while icons/text remain clear, exercise clicks, Start/search, and auto-hide, then restore the system background. Subsequently test monitor hotplug, Explorer recreation, theme/DPI changes, repeated start/stop, and controlled termination of the owning helper. Failed initialization or restoration must remain visible. Existing user processes must not be terminated to make a smoke test pass.

The native component can remain pinned until Explorer exits. Compatible resident reuse preserves its original brushes across helper restart; a loaded incompatible module is never replaced, and a live helper still wins through the per-user mutex/process-handle guard. Manual acceptance must include rapid off/on during connecting/restoring, repeated helper restart without restarting Explorer, temporary taskbar recreation, competing tools starting/stopping, sleep/resume, and successful restoration for each supported material. The test runner must not restart Explorer automatically. Isolated native attachment tests do not prove XAML rendering or full product upgrade/uninstall behavior.

## Installation Acceptance

Run NSIS install/uninstall/upgrade automation only in an isolated account or CI runner with no existing installed or running product. An isolated application-data directory does not isolate HKCU installation registrations and shortcuts. On a workstation with 0.6.3 installed, do not automatically replace or uninstall it for the 0.6.4 local candidate; hand the final setup to the user for first-stage installation/upgrade acceptance.

After building and auditing the artifacts, run:

```powershell
npm run desktop:install-smoke
```

The gate silently installs the current-user NSIS package below a disposable directory, verifies the installed executable, repository-root `LICENSE`, and complete helper payload, and launches it with isolated application/WebView data. It invokes the real uninstaller while the desktop app and its helper are still running, requiring the named shutdown event to produce a graceful helper log, exit both processes, and close port `56873` before the install directory is removed. It then reinstalls, starts a helper from a separate uTools-owned payload/data path, confirms the desktop reports `HELPER_INSTANCE_CONFLICT`, uninstalls the desktop, and requires that external helper and its port to remain alive until the test stops it through authenticated IPC. Both passes require the matching HKCU uninstall entry, current-user shortcuts, install directory, and test processes to be removed. The portable package is exercised separately by the three runtime smoke commands. Artifact inspection requires the repository-root `LICENSE` and generated `THIRD-PARTY-NOTICES.txt` in the portable directory, portable ZIP, and NSIS payload; it verifies representative npm/Cargo components and MIT, Apache, BSD, and MPL terms, rejects the project PolyForm text inside third-party notices, rejects repository-private files, credentials, user configuration, logs, `node_modules`, Rust `target`, source caches, or undeclared binaries, and scans every tracked or untracked non-ignored public source file for credentials before the source is committed.

The install gate first runs `scripts/read-text-file-with-retry.test.ps1`, which holds a synthetic helper log with `FileShare.None` and requires the shared reader to recover after the lock is released. Runtime and install scripts retry only transient `IOException` reads within a bounded deadline; readiness markers, process identity, port closure, uninstall cleanup, and helper-ownership assertions remain strict.

A clean clone of this repository must reproduce the desktop build. Manual Windows checks remain required for tray behavior, optional startup, global input, hot zones, gestures, drag, edge hiding, screenshots, OCR, and topmost controls. macOS checks must include permission prompts and supported window/screenshot actions; Linux checks must run under X11 and verify explicit Wayland degradation. Passing unit tests or a cross-compile alone is not release evidence.

There are only two user acceptance phases. Daily `develop` acceptance normally uses the host integration; build and hand off a local NSIS package only when the user explicitly requests desktop synchronization. Before release, rebuild from clean `main`, publish the final installer and portable archive as a GitHub Pre-release, and test the public downloads end to end. While the Release remains a Pre-release, a failed candidate may be refreshed under the same version by replacing the complete asset set and re-running public download acceptance. The final manifest must identify the clean `main` commit and `SHA256SUMS` must match both deliverables. Stable promotion is allowed only for the exact accepted remote assets; once stable, changed binaries require a new patch version and tag.

Frontend and helper tests share the schema migration and error-code fixtures under `tests/fixtures/`. They cover user overrides, action preservation, repeat migrations, unknown error codes and incompatible protocol/schema rejection.

Windows upgrade acceptance uses `scripts/desktop-upgrade-smoke.ps1 -LegacyInstaller <0.6.x-setup.exe> -Installer <new-setup.exe>`. Run it in an account without an installed or running copy. It backs up stale product registration, installs into a temporary custom Unicode path, verifies schema 7 to 8 migration and its backup with the real packaged app, then checks reinstall, uninstall and user-file retention. Windows CI downloads the immutable 0.6.2 release for this gate. Startup migration unit tests use isolated temporary registry keys and cover disabled-state preservation and repeat runs.


For edge hide, `animationEnabled` defaults to true. Verify explicit false survives import and serialization, performs a single final move with no frame delays, and preserves the configured hide/restore delay and original topmost state. Animated movement keeps its interpolation path. Frontend Vitest suites cap workers at two to avoid memory pressure; parallelize only independent outputs and serialize real helper/installer integration tests.
## Windows helper permission switching

Run `powershell -NoProfile -ExecutionPolicy Bypass -File scripts/helper-owner-smoke.ps1 -HelperPath apps/desktop/src-tauri/resources/helper/magic-corners-helper.exe` to verify owner-exit cleanup, the explicit stop event, and rejection of a reused/stale owner identity. Use an ordinary-permission shell with no other helper running.

For interactive acceptance, start the desktop normally with an isolated data directory. In the power panel, enable administrator mode and cancel UAC: the helper must reconnect in ordinary mode with its configuration intact. Repeat and approve UAC: only the helper should be elevated. Exercise a shortcut and window movement against an elevated test window, then return to ordinary mode. Confirm a configured command launches without elevation. Repeat with the settings window hidden, then force-exit the desktop and verify no helper or listening port remains. Also check a denied elevation request, another helper occupying the fixed port, and repeated permission switches. Administrator mode improves interaction with elevated desktop windows; protected applications can impose additional restrictions.

## 0.6.4 local candidate acceptance

Runtime-center layout revision (2026-10-08) passed the 165 desktop frontend tests, zero-error/warning Svelte checks and production frontend build. Isolated Chromium host/WebSocket mocks verify two equal-height grey cards with aligned, short unclipped descriptions across Chinese/English and 800/640px layouts, permission controls only in Settings, disabled unknown-permission controls, visible warning/error states, details disclosure, ping, themes and start/stop. Unit regressions retain permission-cancellation, reconnection, failed/unknown state and pending-operation guards at the new entry. This is UI/state evidence, not actual UAC, installation, or scheduled-administrator startup acceptance. Scheduled startup is not included in this integration.


Keep an existing 0.6.3 installation intact. NSIS replacement/uninstall is not an automatic local development action; the user performs the actual candidate install/upgrade, while installer automation belongs in an isolated account/CI.

This is development acceptance only: no Release, tag, stable replacement, or release-freeze claim. Preserve existing 0.6.3 outputs by selecting a separate directory for every package gate, from the repository root:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-desktop.ps1 -ArtifactsDir artifacts/0.6.4
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/audit-desktop-artifacts.ps1 -ArtifactsDir artifacts/0.6.4
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/desktop-runtime-smoke.ps1 -AppPath artifacts/0.6.4/ConvenientWindow-portable/ConvenientWindow.exe
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/desktop-runtime-smoke.ps1 -AppPath artifacts/0.6.4/ConvenientWindow-portable/ConvenientWindow.exe -ExpectConflict
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/desktop-runtime-smoke.ps1 -AppPath artifacts/0.6.4/ConvenientWindow-portable/ConvenientWindow.exe -ForceAppKill
# Isolated account/CI only; do not replace an existing 0.6.3 workstation installation
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/desktop-install-smoke.ps1 -ArtifactsDir artifacts/0.6.4
```

`-ArtifactsDir` is restricted to this repository's artifacts directory. Default npm smoke commands still select root artifacts and may exercise an older package; they are not 0.6.4 evidence unless the paths match. Audit the manifest, checksums, portable payload and `artifacts/0.6.4/convenient-window-0.6.4-windows-x64-setup.exe` before handing off. These are expected paths, not a claim that the files exist or passed. Serialize real helper/install gates; stop the current owned helper normally before changing hosts, never kill by port/name. Both hosts share the single-instance boundary and fixed port 56873.

| Change | Automated regression and manual expectation |
| --- | --- |
| #21 pin order | Check a stationary owner raised above its pin, two overlapping topmost owners, minimize/restore and click-to-unpin. The pin stays above its own window but behind unrelated higher windows. |
| #22 original-pixel pinning | Verify small/large captures, signed coordinates and different DPI. Offset defaults to true (16 px down/right), explicit false survives save/import/reload and covers in place. Large/edge images may extend beyond the screen; manual scaling and original-pixel OCR still work. |
| #23 numeric drafts | Exercise DOM input and configuration readback, not only pure helpers. Valid integers persist; empty/incomplete/out-of-range drafts commit on blur/Enter, old context drafts do not leak across monitor/zone/trigger changes, and temporary displays never gain profiles. |
| #24 early shutdown | Queue stop before engine start and repeat start/stop. Original subscription receives it, cleanup exits normally, no leaked listener or unintended recovery remains. Linux Xvfb lifecycle smoke keeps `-noreset`. |
| #25 permissions and commands | Cover failure-state re-query/unknown, same-owner authenticated broker recovery, cancellation and no duplicate execution. Real UAC cancellation/approval, ordinary command tokens, hidden-settings commands and owner-exit cleanup require interactive acceptance. |

Test both `old helper stopped / replacement failed` and `old elevated helper did not stop`; only a successful state query can confirm the actual permission state. A failed query must show unknown, not false or an old cached true. An ordinary shell/host must remain ordinary even when the helper is elevated. A complete host restart or automatic recovery must not request elevation without a new choice. Another host occupying the instance lock must remain untouched. Repeat on each host adapter; desktop tests do not prove another adapter works.

Preserve 0.6.3 behavior: edge-hide animation on/off, screenshot-selection overlays, taskbar transparent/acrylic/tinted restoration, master-switch stop, tray quit and uninstall. Test negative coordinates, mixed DPI and multiple displays without automatically restarting Explorer. Administrator mode does not support protected processes; scheduled elevated login/startup is not part of issue #20's first phase. Automated totals and real UAC/multi-monitor/taskbar results remain unfilled until obtained from this candidate.

### Evidence boundary for browser mocks and taskbar kernel tests

A real browser with mocked host bridges, WebSocket protocol and UAC outcomes can cover numeric drafts, monitor readiness, pinOffset persistence and failed/unknown permission UI states. It is DOM/state evidence only, not real UAC, native helper elevation, another host adapter's preload/command processes, or installer acceptance. Preserve this distinction when recording candidate results.

[`taskbar-appearance-native-test.cpp`](../helper/native/tests/taskbar-appearance-native-test.cpp) exercises the production security factory in `taskbar-shared-security.h`. It inspects an actual kernel mapping for a Medium mandatory label and only current-user/SYSTEM access SIDs, verifies ordinary-caller write access, and verifies that low-integrity impersonation cannot write. The helper process stays elevated when selected; Everyone is not granted access. These tests do not inject into real Explorer. Record their actual run separately from real transparent/acrylic/tinted rendering, ordinary/elevated/ordinary switches, restoration, old-component adoption and multi-monitor/DPI checks. Neither browser mocks nor kernel-object tests complete interactive acceptance.

Full-App integration tests bound their child process to 30 seconds and the enclosing test to 60 seconds. On memory-constrained machines, serialize validation lanes and use a single Vitest worker rather than running UI compilation alongside native builds; these timeouts do not skip assertions or authorize unlimited retries.
