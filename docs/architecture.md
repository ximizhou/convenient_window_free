# Architecture

## Hot-zone hover hint

Introduced in schema v8, the optional `showHotzoneHint` boolean defaults to true for older settings and preserves an explicit false across normalization and persistence. It controls only the pale translucent hover hint. The engine keeps one shared input/hint frame and submits `None` to the dedicated hot-zone hint channel when disabled, hiding an existing hint without clearing edge-hide or gesture overlays. Detection, per-display actions, wheel accumulation, timings, and settings previews remain independent. Request deduplication is unchanged; the option adds no worker, timer, polling, or file I/O. Protocol v7 is unchanged; current settings use schema v9.

## Per-zone geometry (schema v9)

Each global or per-display hot-zone record may include a `geometry` override: `{kind:"corner",width,height,linked}` for corners (2–128 integer physical px), or `{kind:"edge",thickness,lengthPercent}` for edges (2–48 integer physical px, 10–100 integer percent). Length stays centered. Missing geometry preserves the legacy global thickness and square corners / 40% edges; migration does not materialize overrides. Reset removes only the current zone override, and profile copies own their geometry values. Corner priority is retained even for a 100% edge. Runtime hints and input use the same clipped rectangle, while shared fixtures verify the preview implementation, signed desktop coordinates and small displays. Editing the ratio lock preserves an existing rectangle rather than forcing it square.

Protocol v7 is retained, but the frontend requires an explicit schema-v9 ready handshake before flushing configuration. Old/missing/future schema reports cannot silently apply only legacy geometry. The engine resolves one shared frame per tick for both hint and input paths and bypasses display-ID allocation when no profiles exist. New shape calculations introduce no worker, polling or file I/O. Configuration loading migrates earlier supported schemas to v9 without changing actions or timings; downgrade to old hosts is not supported.

## Product Boundary

```text
Standalone desktop (Tauri 2 + Svelte) ----\
                                           > shared localhost protocol -> Rust helper
Host integration (Svelte + preload) --------/
```

This repository is authoritative for the standalone desktop and the helper. Host integrations consume it as a submodule and supply only their host-specific adapters.

## Experimental Taskbar Appearance

The Appearance navigation item opens a shared `TaskbarAppearance.svelte` panel. Global settings remain in the top-right circular gear control between the master switch and theme control. The panel previews transparent, acrylic and tinted materials, but the preview never applies a system change; activation is direct and the Explorer risk note stays in the expandable prototype notes.

Schema v8 adds optional `taskbarAppearance.enabled`, `mode`, `opacity`, `tint` and `showBorder`, defaulting to a disabled transparent preset. Actual activation requires both the global master switch and this opt-in. `helper/src/platform/taskbar.rs` owns a dedicated native worker thread, while `helper/native/taskbar-appearance.cpp` selects an independently implemented Windows x64 backend: XAML Diagnostics for Windows 11 22H2+ and a runtime-resolved `SetWindowCompositionAttribute`/`ACCENT_POLICY` compatibility path for Windows 10 1903 through Windows 11 21H2. Input/window state machines do not wait for either backend initialization. The optional `taskbar.status` event, introduced in protocol v6 and retained in current v7, reports `{ state, available, materials, backgrounds, backend?, errorCode?, terminal?, retryable? }` separately from durable configuration acknowledgements. An applied state only confirms a successful background policy/brush write, not visual compatibility.

The Windows x64 build uses MSVC and the Windows SDK to compile a static-runtime DLL, embeds it in the helper executable, and materializes a content-addressed immutable DLL in the helper data directory only after opt-in. No additional release asset or download allow-list entry is required. Other platforms retain an explicit unavailable state.

Transparent mode clears the background without tint; solid mode uses a tint brush with user-selected opacity, while acrylic mode uses `AcrylicBrush` with `Backdrop` and a separate tint-opacity parameter. Legacy enabled-only configuration preserves the original transparent background and hidden border. Material capability discovery prevents an older boolean-only helper from claiming support for the new modes. Shared parameters are read as coherent bounded snapshots.

The component locates background/border rectangles under `Taskbar.TaskbarFrame`, saves their original brushes, queues changes on the owning UI thread, and requests restoration on disable, normal shutdown, or controller process death. Disable is asynchronous and retains the controller for re-enable; only shutdown/rebind detaches. A compatible pinned resident can adopt a new helper's v2 mapping through its exported attach entry (or the known 0.6.3 v2 hook), preserving the same watcher and original brushes. Only a marker that no longer identifies a loaded module may be removed; a loaded incompatible component is rejected rather than creating a second watcher. The per-user mutex and resident owner-process handle guard live conflicts. Known TranslucentTB/Windhawk processes stop application with an explicit conflict state; retry after the competing tool exits. Explorer PID or taskbar-window recreation triggers rebind. Temporary initialization failures use four bounded backoff retries and a `recovering` status; exhausted, unsupported, conflict, component-missing and connection-timeout states remain terminal errors until an explicit retry. Taskbar recovery never requests elevation, stops other tools, or restarts Explorer.

This is a disabled-by-default technical prototype. Native compilation and the disabled ABI path are tested; real Windows 10 classic rendering, Windows 11 XAML transparency/acrylic rendering, icon clarity, multi-monitor/auto-hide behavior, Explorer restart, and abnormal-exit restoration still require explicit runtime acceptance. The classic path restores the system-default Accent policy on disable as a best-effort fallback because the private API has no reliable cross-process original-policy getter. Classic enumeration, application and restoration are restricted to the resident Explorer PID; dirty notifications are forwarded to the verified resident module after helper replacement. Its border is system-managed and the UI disables the XAML-only border option. Backend capability is queried once from the OS build and reported as optional `backend: "classic" | "xaml"`, rather than advertising materials on every Windows build. Theme brush reinitialization is observed, but visual compatibility must not be inferred from successful API calls. Transient failures use a bounded retry budget; unsupported, conflict and exhausted states stop automatically and require an explicit retry.

### Administrator controller / ordinary Explorer IPC (0.6.4)

A mapping created with default object security by an elevated helper could inherit a high integrity label, preventing medium-integrity Explorer from opening the read/write `FILE_MAP_ALL_ACCESS` acknowledgement channel. [`helper/native/taskbar-shared-security.h`](../helper/native/taskbar-shared-security.h) supplies the production mapping and same-user control lock with an explicit current-user + SYSTEM DACL and a Medium `NO_WRITE_UP` integrity label. It changes only these IPC objects, not the helper process token or executable permissions, and does not grant Everyone access. Failure to construct that descriptor is an initialization error, not a reason to fall back to a permissive ACL.

The native regression checks the actual kernel mapping label/allow-list, ordinary-caller writes, and denied low-integrity impersonation writes. It does not inject into real Explorer. Passing an object-security test does not establish XAML rendering, administrator-switch restoration, or mixed-DPI/multi-monitor acceptance. These remain interactive checks.

## Source Layout

- `apps/desktop/`: reusable Svelte UI, typed host bridge, and Tauri 2 host.
- `helper/`: platform-independent core, IPC, storage, and platform adapters.
- `helper/src/platform/windows/`: complete Windows implementation.
- `helper/src/platform/macos.rs`: macOS Accessibility, Core Graphics, and permission-gated input/capture boundary.
- `helper/src/platform/linux.rs`: Linux X11/EWMH boundary; Wayland is detected and degraded.
- `scripts/`: reproducible development, packaging, smoke-test, and artifact-audit entry points.

Platform-specific behavior stays behind the helper adapter boundary. Core code must not accumulate host checks or new Linux/macOS conditional branches as a substitute for platform adapters. The `helper.ready` payload includes `platform.system`, `platform.architecture`, `platform.session`, and boolean capability fields (`globalInput`, `windowControl`, `windowTopmost`, `screenCapture`, `ocr`, `audio`, `systemActions`, `edgeHide`). Hosts must display unavailable capabilities and continue only with actions the helper reports as supported.

## Host Bridge

Shared UI code depends on a typed bridge for lifecycle, configuration, token access, file dialogs, external links, diagnostics, and host actions. Protocol v7 emits `host.action` with generic kinds and values; the uTools adapter maps redirect actions to its host API. The helper accepts the legacy `utools-redirect` configuration value as an alias, while normalized helper state uses `host-action`. The standalone adapter hides uTools-only actions while preserving unknown action records loaded from schema v8 configuration.

## Configuration Ownership

The shared schema v8 `edgeHide.keepExpandedWhenForeground` setting defaults to `false`, so a collapsed-or-expanded window follows the normal restore delay. When the setting is enabled, an expanded edge-hidden window stays open while it is still the active foreground window. `edgeHide.showRestoreHint` defaults to `true`; disabling it hides only the pale collapsed-window outline while preserving the pointer restore hotzone. Missing fields preserve the prior behavior, while an explicit `false` survives host normalization and helper deserialization.

Edge-hide movement uses a short ease-out transition rather than an instantaneous rectangle jump; intermediate frames preserve the restored window size and the final frame reapplies its original topmost state. A transient foreground surface or a non-maximized surface covering a monitor (for example a screenshot-selection overlay) does not start the expanded-window leave timer, and a cursor inside the live expanded window clears that timer even when foreground-hold is disabled. Shutdown restoration remains immediate so lifecycle cleanup is not delayed by animation.

Startup UI queries carry a generation so a late read cannot overwrite a newer write or refresh. If both a tray write and its state re-query fail, the tray returns to its last confirmed selection and the settings window shows the error rather than implying success.

## Interface Language

`apps/desktop/src/i18n.ts` is the single source of truth for every user-visible string in the desktop settings window. The same module is mirrored byte-for-byte in the uTools plugin front end, so both hosts show identical copy and neither can drift on its own; the "no bare Chinese text nodes" rule for Svelte templates is what keeps English from silently regressing.

- The interface language follows the Windows display language when the user has never chosen one (`zh*` → Chinese, everything else → English) and the explicit choice is stored under the shared `convenient-window-language` localStorage key.
- Status text is stored as dictionary keys (for example `"helperSync"`) and translated at render time, so switching the language also retranslates the message already on screen.
- Protocol 7 reports errors as `{ code, details, requestId? }` in `runtime.error` and `adjustment.updated.error`. Hosts translate the stable `code`, use a generic localized fallback for unknown codes, and keep `details` for diagnostics. Request failures return only to the requesting socket. The error-code contract is covered by `tests/fixtures/runtime-errors.json`.

`helper.ready` reports `protocolVersion: 7` and `schemaVersion: 9`. A `config.update` payload contains `{ protocolVersion: 7, revision, config, gestureLabels? }`; `config.schemaVersion` must be 9. Other versions are rejected before storage or application. `config.applied` retains `requestId`, `revision` and `adjusted`. `gestureLabels` maps configured IDs to translated overlay labels, is capped at 80 non-control characters per label, and is excluded from storage. Hosts resend it on reconnect or language changes.

Schema 8 identifies built-in gestures by their five reserved IDs; an absent `name` selects the dictionary default and an explicit name overrides it. Schema 7 and earlier remove only names matching a reserved ID and a known historical default; other names and actions survive. Those schemas did not record whether a default name was explicitly chosen. The migration is idempotent and rejects future schemas before normalization. Shared migration cases live in `tests/fixtures/i18n-migration.json`. Historical-name rules remain frozen while old settings imports are supported; new translations never enter that table. `gesture.recognized.name` carries an override or `null`; hosts resolve display names by ID. `runtime.status` uses `gesture_not_recognized` or `window_topmost_changed`, with `{ title, topmost }` parameters for the latter.

The desktop host and helper never write the same file:

- The desktop host is the only writer of `<app-data>/desktop-settings.json`. This is the UI's authoritative settings snapshot.
- The helper is the only writer of `<app-data>/helper-data/config.json`. This is the last runtime configuration it accepted.

The UI awaits a successful durable desktop-settings write before sending that exact revision to the helper. Migration uses the same atomic writer and rolling `.bak` file; later saves replace that backup. A failed write is visible to the UI and is never applied to the helper. On first launch after upgrading from the early shared-file layout, the desktop host copies the legacy helper configuration into `desktop-settings.json` only when the new file does not exist; it never overwrites an existing desktop settings file.

## Gesture settings navigation (0.6.4)

Gesture settings use the library as the single selection surface and its New button for creation; no redundant three-card overview is rendered. Removing those shortcuts does not change templates, recording, recognition, action binding, or screenshot settings, and adds no runtime work.

## Number drafts and pinned-image placement (0.6.4)

`number-setting.ts` separates incomplete numeric input from normalized settings. Valid in-range integers persist through the existing path; empty, incomplete, or out-of-range input remains a draft until blur or Enter. Changing display, hot zone, or trigger discards the old context draft. Monitor-specific hot-zone controls wait for an actual helper-reported display identity instead of writing a temporary placeholder profile. Cold-start editing can therefore be briefly unavailable.

Schema v8 adds optional `ocr.pinOffset`, default true when absent; host normalization also defaults invalid input to true, while the helper still requires a boolean; explicit false survives both host normalizers and helper deserialization. Pinned images start at the capture's original physical-pixel dimensions and signed virtual-desktop origin. True offsets the origin 16 px down/right; false covers the capture in place. No initial fit-to-screen scaling is applied, so large or edge-adjacent images may extend beyond the screen. Manual move/resize remains available; OCR still reads the original pixels. Topmost pin tracking checks stacking order even without a position change and keeps a pin immediately above its owner, not above unrelated higher windows.

## Runtime Center

The runtime center presents a compact running/off/starting/disconnected/error/missing state and two aligned grey master/helper cards with short descriptions. Permission controls are only in the top-right Settings panel. Technical platform, version, path and diagnostics data are in an initially collapsed details section. Errors and unknown permissions stay visible outside that section. Feature controls use the same lifecycle operation as the master switch. The login administrator-request preference is separate from the current helper permission state.

## Helper Lifecycle

Each host starts the helper with an absolute `--data-dir` owned by that product. Authentication tokens, runtime configuration, usage data, and logs remain separate between products. A platform-native single-instance lock (the `Global\ConvenientWindowHelper` mutex on Windows and an exclusive runtime/cache lock file on Unix) prevents both products from running helpers concurrently; the losing process logs `HELPER_INSTANCE_CONFLICT` and exits nonzero so the host can show a stable error.

The engine keeps its original shutdown receiver from construction and checks queued shutdown before input-hook initialization and each processing cycle. Re-subscribing at startup would discard an early authenticated stop; startup/stop races must not wait for another notification.

The desktop package resolves a platform-native helper payload. Windows includes the helper EXE and all GNU runtime DLLs required by that exact build; macOS and Linux use an executable Unix helper and never reuse Windows paths or launch commands. On Windows, the Tauri process assigns ordinary-permission desktop-owned helpers to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; elevated helpers use the owner-handle/creation-time and stop-event boundary below; Unix hosts use the same authenticated stop/ownership boundary without assuming a Windows process primitive. Cleanup never searches for or kills helpers by executable name, so a separately owned uTools helper is outside the desktop lifecycle boundary.

The NSIS pre-uninstall hook signals `Local\com.ximizhou.convenientwindow.shutdown` and waits briefly for the desktop process to use the same guarded shutdown path before files are removed. Tray quit, Tauri exit events, and uninstall converge on a one-time shutdown guard. Closing the main window only hides it. Optional startup registration is exposed in Settings and as the checked start-at-login tray item; both use the same native operations. Autostart launches with `--autostart` and keeps the settings window hidden. Windows can add the separate login administrator request described below.

Automated runtime tests may set the absolute `CONVENIENT_WINDOW_DATA_DIR` override. In that mode desktop settings, helper data, and WebView data all stay below the explicit root. Production startup uses the platform application-data path.

Edge-hide state keeps the target edge and restore geometry across monitor changes. The helper enables a restore strip and its pointer hotzone only when the restore rectangle still intersects the current monitor topology, its edge remains exposed on the virtual desktop, and a visible live window still matches the hidden rectangle. Empty or changed monitor snapshots, failed platform queries, externally moved, hidden, or minimized windows, and removed displays therefore cannot leave either a stale pale outline or an invisible hotzone. Initial and repeated collapse commands remain unconfirmed until that live-geometry check succeeds; an expand command likewise remains unconfirmed until the live window reaches its restore rectangle. A mismatch enters a hint-free cleanup state, and cleanup or batch-restore failures back off and retry instead of losing the original topmost state. After a topology change, the helper relocates a window that remains at its old collapsed geometry, including when an added display turns the old outer edge into a seam, or adopts the expected new geometry when Windows already moved it. A relocation is committed only after the same check; failures back off without blocking other windows. Disabling or stopping the engine reclamps restore geometry, makes up to three immediate recovery attempts, and explicitly clears the final rendered hint frame.
Window-drag admission is split into a read-only low-level hook check and an engine-side resolver. The hook checks the pointer target against `windowDrag.pausedApps` before consuming the configured modifier+mouse press; a blocked target receives the original press, motion, and release unchanged. Only an allowed target reaches the engine resolver, which may restore a maximized window and start the drag session. The hook check must never mutate window geometry.

## Brightness Controls

The `brightness-adjust` action uses presets of `0.05` and `-0.05`, measured against the device's reported brightness range with a minimum step of one hardware unit. Hot zones select the display containing the trigger point; gestures preserve their target point. Wheel and slide triggers use the existing continuous-action scaling.

A background worker merges consecutive deltas for the same display and direction. Reversals retain their order so movement away from a hardware limit remains effective. Hardware access stays outside the input loop; pending state, successful readback, and failures return through `adjustment.updated`. External commands use argument arrays, bounded output, a five-second timeout, and a process group that is cleaned up on completion or timeout.

| Platform | Internal or system-controlled display | Other external displays |
| --- | --- | --- |
| Windows | WMI when supported; DDC/CI is the peer fallback | DDC/CI through Windows monitor APIs; if WMI and DDC/CI both fail, use per-display GDI gamma software fallback (70–100%) |
| Linux X11 | sysfs backlight reads and logind `SetBrightness`; requires `busctl`, systemd-logind, and an authorized local session | `ddcutil`, VCP `0x10`; install with `sudo apt install ddcutil` on Debian/Ubuntu, enable DDC/CI, and configure [I²C permissions](https://www.ddcutil.com/i2c_permissions/) |
| macOS Intel | DisplayServices loaded at runtime | Native IOKit I²C/DDC |
| macOS Apple Silicon | DisplayServices loaded at runtime | [m1ddc](https://github.com/waydabber/m1ddc) 1.2.0 or newer; install with `brew install m1ddc` |

On Windows, WMI and DDC/CI are peer hardware candidates rather than a strict priority chain. When both are unavailable, the helper applies a per-display GDI gamma ramp as a visual fallback, clamps it to 70–100% because common Intel/Windows drivers reject lower peaks, labels the feedback as software brightness, and restores the captured ramp on normal helper shutdown. This does not replace physical backlight control; enabling DDC/CI in the monitor OSD remains preferred.

Linux matches the selected RandR output's EDID to exactly one connected DRM connector and uses that connector's DDC bus. Internal backlights must belong to the connector or its GPU, with exactly one backlight and one connected internal panel on that GPU. Ambiguous matches, missing EDIDs, virtual outputs, and cloned outputs produce errors. Backlight access follows the [kernel ABI](https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-class-backlight) and [logind interface](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).

macOS addresses the selected `CGDisplayID` and rejects mirrored displays. DisplayServices is a private interface requiring regression checks after OS updates. Intel DDC validates replies and requires a unique responding I²C bus on the display's framebuffer. Apple Silicon searches `/opt/homebrew/bin/m1ddc`, `/usr/local/bin/m1ddc`, then `PATH`, and passes `display id=<CGDisplayID>` on every read/write.

Brightness test coverage and hardware acceptance requirements are in [Testing](testing.md#brightness-controls).

## Adjustment feedback

Volume and brightness share the helper's `platform::adjustment` queue and result contract. They have separate workers so display-driver latency cannot block audio. Brightness retains the selected monitor identity; audio uses the default output and binds read, write, and readback to that same device. Consecutive inputs merge only when their display and direction match. Each queue is bounded to 32 entries, and requests waiting longer than one second expire before touching a device.

Windows reuses a worker-local `IAudioEndpointVolume` and COM apartment. Device notifications invalidate the cached output on default-device, connection, state, or name changes; failures discard the session so the next input reconnects without replaying a possibly completed write. macOS uses Core Audio HAL master volume or the device's writable channel volumes. Linux requires `pactl` with JSON output and a PulseAudio-compatible server, including PipeWire's Pulse service. Linux/macOS channel updates preserve the existing balance. All backends clamp application volume changes to 0–100%; increasing volume clears mute, while decreasing it preserves mute. Device lookup and write errors propagate to the indicator.

`adjustment.updated` carries `interaction`, `sequence`, `kind` (`volume` or `brightness`), the trigger screen's `screen` bounds, `pending`, an optional `level` (`value`, `muted`, `deviceName`), and an optional `error`. `value` is the device readback normalized to its range. Each completed readback in the active interaction updates the indicator even while newer input is queued. Pending events retain that readback, including when input arrives before the engine polls the result. Switching control or monitor, or resuming after 1.2 seconds without input, starts a new interaction that rejects earlier results. `sequence` orders feedback events within the helper process.

A failure is reported twice on purpose: the helper sends `adjustment.updated` with `error` **and** keeps the classic `runtime.error` message, because the uTools plugin renders status text rather than an indicator window and would otherwise lose the reason silently. Hosts that show the indicator ignore the duplicate; the wording of both paths comes from the shared dictionary (`adjustmentFailed`, `adjustmentMuted`, `adjustmentPending`, `adjustmentReading`, `adjustmentVolume`, `adjustmentBrightness`).

The Tauri Rust host maintains its own authenticated subscription while its managed helper runs. A dedicated Svelte entry point (`hud.html`) renders one hidden, non-focusable, click-through window. The settings WebView does not route these events. Host revisions order live events, initial snapshots, expiry, and helper reconnects; the frontend acknowledges rendering before the host shows the window. First-use pending feedback stays hidden for 250 milliseconds; readbacks and failures appear as soon as rendered. Repeated input in the same interaction does not restart this delay. The indicator appears near the bottom of the trigger display, remains for 1.2 seconds after success or 3 seconds after failure, and updates in place. Pending feedback expires after 15 seconds. The indicator responds only to this application's actions.

Repeated feedback with unchanged visible content extends the deadline without another WebView update. Monitor placement is resolved for a new interaction, screen, or scale factor; native size, position, and visibility calls run only when those values change. Idle timer checks stay off the main thread. The progress bar uses a transform transition so intermediate animation frames do not change layout.

The current Linux input and monitor backend requires X11. Wayland support needs its own input and overlay integration. macOS window positioning and fullscreen behavior still require native machine verification.

## Platform and Release Boundary

| Host | Runtime boundary | Acceptance status | Explicitly unavailable |
| --- | --- | --- | --- |
| Windows 11 x64 | Complete helper and desktop behavior, including OCR, edge hiding, and topmost controls | Release-accepted | None in the current P0 scope |
| macOS x64/arm64 | Accessibility-gated global input/window control; Core Graphics monitor and screen capture; Core Audio volume via `platform/macos_audio.rs` | Cross-compile check; native permission, audio, and window smoke pending | OCR, edge hiding, arbitrary-window topmost |
| Linux x64 X11 | X11 global input/window control, RandR monitors, EWMH topmost, X11 capture, volume through `pactl` | Cross-compile check; native X11 runner and audio smoke pending | OCR, edge hiding |
| Linux Wayland | Session detection and capability reporting only | Degradation behavior tested; no false-ready support claim | Global input and arbitrary-window control unless a future portal path is proven |

The package produces a per-user NSIS installer and a portable archive for the currently accepted Windows target. macOS/Linux assets stay out of release manifests until native runner and real-machine acceptance records their exact binary, size, and SHA-256. Public GitHub Releases use immutable final-version assets: a clean `main` build is published as a Pre-release for online acceptance, then promoted in place. Automatic updates and trusted commercial code signing are not implemented.

The product name is `Convenient Window`; the identifier `com.ximizhou.convenientwindow`, executable name and application-data directory remain stable. Windows upgrades validate the legacy registration and keep the existing installation directory, then remove the old uninstall entry and matching shortcuts after installation succeeds. Startup migration accepts only entries targeting that same executable path, preserves Task Manager's disabled state, writes the new entry before deleting the old one, and can retry after interruption. Registry migration failure does not block application launch. These legacy identity rules remain isolated in the Windows adapter and installer hooks.


Edge-hide animation is controlled by the optional `edgeHide.animationEnabled` preference (default true). Disabled transitions write the final geometry/topmost state once; cleanup transitions are always immediate. Trigger conditions, collapse/restore delays and the controller acknowledgement flow do not change.
## Optional administrator helper on Windows

The desktop remains at ordinary user permissions. The administrator control under top-right Settings can restart its helper with `ShellExecuteExW` and the `runas` verb. This mode switch lasts for the current helper session and does not change the separate login preference; automatic recovery starts an ordinary helper without requesting UAC. Cancellation or a confirmed failed elevated launch restarts the ordinary helper and reports the cancellation/fallback. A replacement is started only after the previous owned process has exited. An unconfirmed elevated launch (`adminLaunchUnconfirmed`) blocks fallback and further starts rather than risking a duplicate helper or repeated UAC.

The authenticated protocol-v7/schema-v9 `helper.ready` message includes `processId`, `elevated`, and `desktopManaged`. These report the actual helper PID, token elevation, and enabled owner binding; the retained `desktopManaged` name is not a product-identity assertion. The supervisor checks the owned process ID, authentication, and protocol before accepting readiness or issuing shutdown. Permission-switch errors query actual running/elevation state again; if that query fails, the frontend uses unknown (`elevated: null`) rather than a previous cached boolean. An unconfirmed state is not evidence that an old elevated helper stopped. An ordinary helper remains in the desktop's kill-on-close Job Object. An elevated helper monitors the desktop process handle and creation time, and listens for a per-launch stop event. Owner exit or that event requests graceful shutdown, with a bounded exit fallback if runtime cleanup stalls.

Managed helpers send `desktop.command` events instead of executing configured commands. One authenticated native desktop listener handles those events, independently of the settings WebView, and launches commands with ordinary permissions. It must reconnect only to the same owned helper PID, honor cancellation during reconnect, and avoid duplicate execution. `desktop.command.data` is `{ "command": "..." }`; it is not a public elevation API. In this route `action.triggered` records dispatch, not successful child-process completion. An elevated standalone helper rejects command execution. When the desktop itself was manually elevated, existing window/input features retain inherited permissions, while permission switching and command execution are refused. Launch the desktop normally to use those features.

Administrator mode is Windows-only and does not support protected processes. The current helper mode is not stored in the settings schema; the Windows login request is stored separately in startup registration. No scheduled task is created; startup launches the ordinary-permission desktop, which requests helper elevation through UAC. Other host adapters must independently verify their ordinary-permission command broker and owner-lifetime integration; the desktop implementation alone is not their runtime evidence. The single-instance lock and fixed port `56873` remain shared, so permission switching never takes over another host's helper. UAC, multi-monitor/DPI, and taskbar restoration remain real-machine acceptance items for the 0.6.4 local candidate.

### Login administrator request and startup registration

The Windows desktop preference **Request administrator access at login** is off by default. Enabling it writes the current executable's quoted command with `--autostart --request-admin` to the current-user `Run` entry named `Convenient Window` and also enables startup. Disabling only the request changes an owned command back to `--autostart` without removing startup or clearing Task Manager's disabled override. These registry changes do not switch the running helper's mode and do not invoke UAC.

Only a desktop process launched with both arguments arms a one-shot login request. The first helper-start request consumes it and uses the existing guarded permission-switch path. Later requests do not rearm it; automatic recovery creates a new helper in ordinary mode, while an already running owned helper is reused in its actual mode. A foreground launch, `--autostart` alone, or `--request-admin` alone does not arm the request, even when startup registration contains the preference. A cancelled or confirmed failed request may recover an ordinary helper, but an unconfirmed launch or unconfirmed previous-process exit cannot start a replacement. The preference requests authorization at startup; it does not bypass UAC or make the interface, command broker or subsequent recovery elevated.

Startup readback checks both the registered executable command and Windows `StartupApproved\Run` state. Task Manager can disable startup while leaving `--request-admin` in the `Run` command. Re-enabling startup through the app retains that owned command's administrator argument and clears its disabled override instead of deriving the preference from the inactive checkbox state. In contrast, explicitly disabling startup in the app removes the matching `Run` and `StartupApproved` values, so a later plain re-enable defaults to an ordinary login start.

Removal is scoped to a recognized startup command whose absolute executable path matches the current executable, case-insensitively. Disabling startup from another installed or portable copy must leave the registered copy's command and approval value untouched. The per-user entry name is shared; this is an ownership check, not independent startup registration for every copy. Registry regressions use isolated temporary keys to exercise administrator-argument preservation, disabled-state handling, owned removal, repeat removal and other-copy preservation without altering production startup entries.

Native startup writes are serialized and run off the UI thread. Updates refresh the tray checkmark and emit `startup-changed`; Settings rereads startup and login-request state after changes and on window focus. Read failures remain unknown rather than being shown as disabled or as an old successful value. Tray quit, desktop exit and owner death retain the same owned-helper cleanup boundary for both permission modes. The [first-stage manual checklist](testing.md#login-administrator-request-first-stage-manual-acceptance) is still required; registry and mocked-UI tests do not establish real-user UAC, actual login, Windows 10 compatibility or packaged upgrade/uninstall acceptance.
