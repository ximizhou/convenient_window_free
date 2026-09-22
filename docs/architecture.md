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
- `helper/src/platform/windows/`: complete Windows implementation.
- `helper/src/platform/macos.rs`: macOS Accessibility, Core Graphics, and permission-gated input/capture boundary.
- `helper/src/platform/linux.rs`: Linux X11/EWMH boundary; Wayland is detected and degraded.
- `scripts/`: reproducible development, packaging, smoke-test, and artifact-audit entry points.

Platform-specific behavior stays behind the helper adapter boundary. Core code must not accumulate host checks or new Linux/macOS conditional branches as a substitute for platform adapters. The `helper.ready` payload includes `platform.system`, `platform.architecture`, `platform.session`, and boolean capability fields (`globalInput`, `windowControl`, `windowTopmost`, `screenCapture`, `ocr`, `audio`, `systemActions`, `edgeHide`). Hosts must display unavailable capabilities and continue only with actions the helper reports as supported.

## Host Bridge

Shared UI code depends on a typed bridge for lifecycle, configuration, token access, file dialogs, external links, diagnostics, and host actions. Protocol v6 emits `host.action` with generic kinds and values; the uTools adapter maps redirect actions to its host API. The helper accepts the legacy `utools-redirect` configuration value as an alias, while normalized helper state uses `host-action`. The standalone adapter hides uTools-only actions while preserving unknown action records loaded from schema v7 configuration.

## Configuration Ownership

The shared schema v7 `edgeHide.keepExpandedWhenForeground` setting defaults to `true`. An expanded edge-hidden window remains open while it is still the active foreground window; when the setting is disabled, leaving the window starts the normal restore-delay countdown even if that window remains foreground. `edgeHide.showRestoreHint` also defaults to `true`; disabling it hides only the pale collapsed-window outline while preserving the pointer restore hotzone. Missing fields preserve the prior behavior, while an explicit `false` survives host normalization and helper deserialization.

The desktop host and helper never write the same file:

- The desktop host is the only writer of `<app-data>/desktop-settings.json`. This is the UI's authoritative settings snapshot.
- The helper is the only writer of `<app-data>/helper-data/config.json`. This is the last runtime configuration it accepted.

The UI awaits a successful durable desktop-settings write before sending that exact revision to the helper. A failed write is visible to the UI and is never applied to the helper. On first launch after upgrading from the early shared-file layout, the desktop host copies the legacy helper configuration into `desktop-settings.json` only when the new file does not exist; it never overwrites an existing desktop settings file.

## Helper Lifecycle

Each host starts the helper with an absolute `--data-dir` owned by that product. Authentication tokens, runtime configuration, usage data, and logs remain separate between products. A platform-native single-instance lock (the `Global\ConvenientWindowHelper` mutex on Windows and an exclusive runtime/cache lock file on Unix) prevents both products from running helpers concurrently; the losing process logs `HELPER_INSTANCE_CONFLICT` and exits nonzero so the host can show a stable error.

The desktop package resolves a platform-native helper payload. Windows includes the helper EXE and all GNU runtime DLLs required by that exact build; macOS and Linux use an executable Unix helper and never reuse Windows paths or launch commands. On Windows, the Tauri process assigns every desktop-owned helper to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`; Unix hosts use the same authenticated stop/ownership boundary without assuming a Windows process primitive. Cleanup never searches for or kills helpers by executable name, so a separately owned uTools helper is outside the desktop lifecycle boundary.

The NSIS pre-uninstall hook signals `Local\com.ximizhou.convenientwindow.shutdown` and waits briefly for the desktop process to use the same guarded shutdown path before files are removed. Tray quit, Tauri exit events, and uninstall converge on a one-time shutdown guard. Closing the main window only hides it. Optional startup registration is exposed only as the checked `开机自动启动` tray item; autostart launches with `--autostart` and keeps the settings window hidden.

Automated runtime tests may set the absolute `CONVENIENT_WINDOW_DATA_DIR` override. In that mode desktop settings, helper data, and WebView data all stay below the explicit root. Production startup uses the platform application-data path.

Edge-hide state keeps the target edge and restore geometry across monitor changes. The helper enables a restore strip and its pointer hotzone only when the restore rectangle still intersects the current monitor topology, its edge remains exposed on the virtual desktop, and a visible live window still matches the hidden rectangle. Empty or changed monitor snapshots, failed platform queries, externally moved, hidden, or minimized windows, and removed displays therefore cannot leave either a stale pale outline or an invisible hotzone. Initial and repeated collapse commands remain unconfirmed until that live-geometry check succeeds; an expand command likewise remains unconfirmed until the live window reaches its restore rectangle. A mismatch enters a hint-free cleanup state, and cleanup or batch-restore failures back off and retry instead of losing the original topmost state. After a topology change, the helper relocates a window that remains at its old collapsed geometry, including when an added display turns the old outer edge into a seam, or adopts the expected new geometry when Windows already moved it. A relocation is committed only after the same check; failures back off without blocking other windows. Disabling or stopping the engine reclamps restore geometry, makes up to three immediate recovery attempts, and explicitly clears the final rendered hint frame.

## Brightness Controls

The `brightness-adjust` action uses presets of `0.05` and `-0.05`, measured against the device's reported brightness range with a minimum step of one hardware unit. Hot zones select the display containing the trigger point; gestures preserve their target point. Wheel and slide triggers use the existing continuous-action scaling.

A background worker merges consecutive deltas for the same display and direction. Reversals retain their order so movement away from a hardware limit remains effective. Hardware access stays outside the input loop; pending state, successful readback, and failures return through `adjustment.updated`. External commands use argument arrays, bounded output, a five-second timeout, and a process group that is cleaned up on completion or timeout.

| Platform | Internal or system-controlled display | Other external displays |
| --- | --- | --- |
| Windows | WMI matched to the monitor instance | Windows monitor APIs; enable DDC/CI in the monitor menu |
| Linux X11 | sysfs backlight reads and logind `SetBrightness`; requires `busctl`, systemd-logind, and an authorized local session | `ddcutil`, VCP `0x10`; install with `sudo apt install ddcutil` on Debian/Ubuntu, enable DDC/CI, and configure [I²C permissions](https://www.ddcutil.com/i2c_permissions/) |
| macOS Intel | DisplayServices loaded at runtime | Native IOKit I²C/DDC |
| macOS Apple Silicon | DisplayServices loaded at runtime | [m1ddc](https://github.com/waydabber/m1ddc) 1.2.0 or newer; install with `brew install m1ddc` |

Linux matches the selected RandR output's EDID to exactly one connected DRM connector and uses that connector's DDC bus. Internal backlights must belong to the connector or its GPU, with exactly one backlight and one connected internal panel on that GPU. Ambiguous matches, missing EDIDs, virtual outputs, and cloned outputs produce errors. Backlight access follows the [kernel ABI](https://www.kernel.org/doc/Documentation/ABI/stable/sysfs-class-backlight) and [logind interface](https://www.freedesktop.org/software/systemd/man/latest/org.freedesktop.login1.html).

macOS addresses the selected `CGDisplayID` and rejects mirrored displays. DisplayServices is a private interface requiring regression checks after OS updates. Intel DDC validates replies and requires a unique responding I²C bus on the display's framebuffer. Apple Silicon searches `/opt/homebrew/bin/m1ddc`, `/usr/local/bin/m1ddc`, then `PATH`, and passes `display id=<CGDisplayID>` on every read/write.

Brightness test coverage and hardware acceptance requirements are in [Testing](testing.md#brightness-controls).

## Adjustment feedback

Volume and brightness share the helper's `platform::adjustment` queue and result contract. They have separate workers so display-driver latency cannot block audio. Brightness retains the selected monitor identity; audio uses the default output and binds read, write, and readback to that same device. Consecutive inputs merge only when their display and direction match. Each queue is bounded to 32 entries, and requests waiting longer than one second expire before touching a device.

Windows reuses a worker-local `IAudioEndpointVolume` and COM apartment. Device notifications invalidate the cached output on default-device, connection, state, or name changes; failures discard the session so the next input reconnects without replaying a possibly completed write. macOS uses Core Audio HAL master volume or the device's writable channel volumes. Linux requires `pactl` with JSON output and a PulseAudio-compatible server, including PipeWire's Pulse service. Linux/macOS channel updates preserve the existing balance. All backends clamp application volume changes to 0–100%; increasing volume clears mute, while decreasing it preserves mute. Device lookup and write errors propagate to the indicator.

`adjustment.updated` carries `interaction`, `sequence`, `kind` (`volume` or `brightness`), the trigger screen's `screen` bounds, `pending`, an optional `level` (`value`, `muted`, `deviceName`), and an optional `error`. `value` is the device readback normalized to its range. Each completed readback in the active interaction updates the indicator even while newer input is queued. Pending events retain that readback, including when input arrives before the engine polls the result. Switching control or monitor, or resuming after 1.2 seconds without input, starts a new interaction that rejects earlier results. `sequence` orders feedback events within the helper process. Other hosts can render this event independently.

The Tauri Rust host maintains its own authenticated subscription while its managed helper runs. A dedicated Svelte entry point (`hud.html`) renders one hidden, non-focusable, click-through window. The settings WebView does not route these events. Host revisions order live events, initial snapshots, expiry, and helper reconnects; the frontend acknowledges rendering before the host shows the window. First-use pending feedback stays hidden for 250 milliseconds; readbacks and failures appear as soon as rendered. Repeated input in the same interaction does not restart this delay. The indicator appears near the bottom of the trigger display, remains for 1.2 seconds after success or 3 seconds after failure, and updates in place. Pending feedback expires after 15 seconds. The indicator responds only to this application's actions.

Repeated feedback with unchanged visible content extends the deadline without another WebView update. Monitor placement is resolved for a new interaction, screen, or scale factor; native size, position, and visibility calls run only when those values change. Idle timer checks stay off the main thread. The progress bar uses a transform transition so intermediate animation frames do not change layout.

The current Linux input and monitor backend requires X11. Wayland support needs its own input and overlay integration. macOS window positioning and fullscreen behavior still require native machine verification.

## Platform and Release Boundary

| Host | Runtime boundary | Acceptance status | Explicitly unavailable |
| --- | --- | --- | --- |
| Windows 11 x64 | Complete helper and desktop behavior, including OCR, edge hiding, and topmost controls | Release-accepted | None in the current P0 scope |
| macOS x64/arm64 | Accessibility-gated global input/window control; Core Graphics monitor and screen capture | Cross-compile check; native permission and window smoke pending | OCR, edge hiding, arbitrary-window topmost |
| Linux x64 X11 | X11 global input/window control, RandR monitors, EWMH topmost, X11 capture | Cross-compile check; native X11 runner smoke pending | OCR, edge hiding |
| Linux Wayland | Session detection and capability reporting only | Degradation behavior tested; no false-ready support claim | Global input and arbitrary-window control unless a future portal path is proven |

The package produces a per-user NSIS installer and a portable archive for the currently accepted Windows target. macOS/Linux assets stay out of release manifests until native runner and real-machine acceptance records their exact binary, size, and SHA-256. Public GitHub Releases use immutable final-version assets: a clean `main` build is published as a Pre-release for online acceptance, then promoted in place. Automatic updates and trusted commercial code signing are not implemented.
