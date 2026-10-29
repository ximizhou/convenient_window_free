<div align="center">

<img src="docs/images/app-icon.png" width="88" alt="Convenient Window icon">

<h1>Convenient Window</h1>

<p><strong>Turn screen edges, windows, and mouse gestures into faster controls.</strong></p>

<p>Windows 11 x64 · Tauri 2 · Rust · Svelte</p>

<p>
  <a href="https://github.com/ximizhou/convenient_window_free/releases/latest"><img src="https://img.shields.io/github/v/release/ximizhou/convenient_window_free?label=stable&color=3f6ee4&style=flat-square" alt="Stable release"></a>
  <a href="#platform-support"><img src="https://img.shields.io/badge/platform-Windows%2011%20x64-3f6ee4?style=flat-square" alt="Platform"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-PolyForm%20Noncommercial%201.0.0-15915c?style=flat-square" alt="License"></a>
</p>

<p><a href="README.zh-CN.md">简体中文</a> · <strong>English</strong></p>

</div>

> Development 0.6.9 adds [startup application management](docs/startup-apps.md), private diagnostic exports and offline display-profile management. These source changes are not a new published release.


![Convenient Window overview](docs/images/running-center.png)

Convenient Window is a small tray companion for Windows 11. Configure the four corners and four edges of every display, tuck windows away at the screen edge, draw one-stroke mouse gestures, keep screenshots floating above other windows, and save the current desktop as a **window layout** you can restore in one click. Settings apply live, the interface is bilingual, and everything runs locally.

## What's new in 0.6.8

0.6.8 is the current stable release for Windows 11 x64; the 0.6.7 stable downloads remain available for rollback.

- **Language fixes** — English mode now switches every label, status message and helper-install feedback; settings labels, feature entries and helper operations no longer fall back to Chinese.
- **Bottom tabs** — the bottom navigation uses short one-word labels and stays readable with the settings drawer open or in narrow windows; full feature names remain in the tooltips and accessible labels.
- **No behavior changes** — hot corners, edge hiding, drag/resize, gestures, screenshots and OCR, window layouts and the topmost pin behave as before; the helper only bumped its version in this release.

[Release notes](docs/release-notes/0.6.8.md) · [All releases](https://github.com/ximizhou/convenient_window_free/releases)

## A closer look

<table>
  <tr>
    <td width="50%"><img src="docs/images/window-layouts.png" alt="Window layout snapshots and one-click restore" /></td>
    <td width="50%"><img src="docs/images/feature-entries.png" alt="Composing bottom tabs from feature-entry bubbles" /></td>
  </tr>
  <tr>
    <td align="center"><strong>Window layouts</strong><br/><sub>Save the desktop you have, restore it in one click.</sub></td>
    <td align="center"><strong>Feature entries</strong><br/><sub>Compose the bottom tabs with drag-and-drop bubbles.</sub></td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/hot-zones.png" alt="Per-monitor hot-zone configuration" /></td>
    <td width="50%"><img src="docs/images/window-enhancement.png" alt="Window edge-hiding configuration" /></td>
  </tr>
  <tr>
    <td align="center"><strong>Hot zones</strong><br/><sub>Every corner and edge of every display is configurable.</sub></td>
    <td align="center"><strong>Edge hiding</strong><br/><sub>Tuck windows away and restore them from a visible strip.</sub></td>
  </tr>
  <tr>
    <td colspan="2"><img src="docs/images/mouse-gestures.png" alt="Mouse gesture workbench with screenshot and local OCR actions" /></td>
  </tr>
  <tr>
    <td colspan="2" align="center"><strong>Gestures, screenshots, and local OCR</strong><br/><sub>Built-in gestures, custom samples, and editable actions.</sub></td>
  </tr>
  <tr>
    <td colspan="2"><img src="docs/images/dark-theme.png" alt="Convenient Window dark theme" /></td>
  </tr>
  <tr>
    <td colspan="2" align="center"><strong>Light and dark</strong><br/><sub>Follows the system appearance or a manual choice.</sub></td>
  </tr>
</table>

## Features

**Window control**
- **Window layouts**: snapshot and restore ordinary windows, including maximized, minimized and topmost state, with per-window identity checks and skipped-window summaries.
- **Anywhere move and resize**: move or resize the active window with configurable modifier-and-mouse combinations.
- **Topmost controls**: keep a window above others; a red in-window pin releases it with a click.
- **Per-monitor profiles**: independent settings for each display, including signed virtual-desktop coordinates.

**Screen edges**
- **Hot zones**: the four corners and four edges of each monitor, with hover, mouse-button, wheel and edge-movement triggers. The translucent hover hint can be disabled without disabling actions.
- **Zone geometry**: per-display corner width/height with ratio lock, edge thickness, centered length, live preview and per-zone reset.
- **Edge hiding**: move windows partly off-screen and restore them from the visible strip; multi-window and multi-monitor aware. Restore outline and animation can be disabled independently.

**Gestures and capture**
- **Global mouse gestures**: bind one-stroke gestures to shortcuts, system actions, commands and window controls; create and manage custom samples (up to 64 gestures, 8 samples each).
- **Screenshots and pinned images**: capture an area, keep it above other windows, move, resize, adjust opacity, copy or save as PNG.
- **Local OCR**: recognize text through local Windows APIs and copy it without uploading the screenshot.

**Comfort**
- **Brightness and volume**: adjust the triggering display or output from hot zones and gestures, with a passive on-screen indicator.
- **Live settings**: most changes save and apply immediately.
- **Themes and language**: light/dark themes and a Chinese/English interface, switchable any time.

## Download

| Package | Use it when |
| --- | --- |
| [Installer (`setup.exe`)](https://github.com/ximizhou/convenient_window_free/releases/latest) | Recommended for a normal per-user installation. |
| Portable ZIP | Extract and run from any folder; the archive contains `ConvenientWindow-portable`. |
| Requirements | Windows 11 x64 for the published packages. |

The current release assets are **unsigned**, so Windows may show an unknown-publisher or Microsoft Defender SmartScreen warning. Each release ships `SHA256SUMS` and an `artifact-manifest.json` so the download can be verified. If security software blocks `magic-corners-helper.exe`, verify the checksum first, then check the quarantine area or report a false positive rather than disabling protection.

**Updating the portable build**: extract the newer archive over the existing `ConvenientWindow-portable` directory. Settings stay in Local AppData rather than inside the portable folder, so they survive the replacement.

## Quick start

1. Download and install the app, or extract the portable archive.
2. Open Convenient Window from the system tray.
3. Turn on the main switch and enable the features you want in the bottom tabs: hot zones, edge hiding, gestures, window layouts, or appearance.
4. In Settings (gear icon), choose light/dark theme, the interface language, startup behavior, and the poll interval.

Closing the settings window keeps the tray app running; use the tray menu to reopen it or quit.

### How edge hiding works

1. Drag a window toward a screen edge until the blue line appears.
2. Release to tuck the window away. For the top edge, wait until the line fills before releasing; releasing early keeps native Snap.
3. Move the pointer back to the exposed strip to restore the window.

## Privacy and security

- OCR runs through local Windows APIs; screenshots are never uploaded for recognition.
- Configuration, tokens, usage data and logs stay in the local application data directory.
- Local IPC is protected by a random token and a single-instance lock.
- The helper is a separate component that is installed explicitly, verified by size and SHA-256, and can be stopped or removed at any time.

Please report vulnerabilities privately as described in [SECURITY.md](SECURITY.md).

## Platform support

| Platform | Status | Notes |
| --- | --- | --- |
| **Windows 11 x64** | Release-accepted | Full feature set, NSIS installer and portable archive. |
| macOS (x64 / arm64) | Unverified | Native helper boundary for input, window control, monitors and capture; Accessibility and Screen Recording permissions required. OCR, edge hiding and arbitrary-window topmost unavailable. Release assets are not published. |
| Linux X11 (x64) | Unverified | Native helper supports input, window control, monitors, capture and EWMH topmost. OCR and edge hiding unavailable. Release assets are not published. |
| Linux Wayland | Capability detection only | Reports unavailable instead of pretending to support global input or arbitrary-window control. |

The installer and executable are not code-signed, and automatic updates are not implemented. Taskbar material is an off-by-default Windows 11 x64 experiment with transparent, acrylic and tinted modes; the panel keeps the Explorer risk note.

## For developers

<details>
<summary><strong>Build and test from source</strong></summary>

This repository is the source of truth for `apps/desktop/` (Tauri 2 host and Svelte UI), `helper/` (shared Rust helper), and `scripts/` (build, packaging, smoke-test and artifact-audit entry points). Host integrations consume this repository as a submodule and provide only their own host adapters.

Prerequisites: Node.js `24.14.0` (see `.node-version`), Windows 11 x64 for the release build, and Rust `1.96.0`/newer stable. The Windows helper build also needs Visual Studio C++ Build Tools and a Windows SDK with C++/WinRT headers; `scripts/windows-toolchain.ps1` handles discovery and `VSINSTALLDIR` can point at an existing installation.

```powershell
npm ci
npm run desktop:test
npm run desktop:check
npm run desktop:frontend
npm run desktop:build
```

`scripts/build-desktop.ps1 -ArtifactsDir artifacts/<candidate>` preserves previous outputs; the path must stay inside `artifacts/`. Packaged lifecycle and artifact gates are described in [Testing](docs/testing.md); see [Architecture](docs/architecture.md) for the helper protocol and window-state machine.

For helper-only work on Windows:

```powershell
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo fmt --check
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo test
```

</details>

## Release integrity

<details>
<summary><strong>How releases are built and verified</strong></summary>

A clean `main` build is published as a replaceable Pre-release candidate; after user acceptance the exact same assets are promoted to stable without replacement. Every release includes `SHA256SUMS` and an artifact manifest tied to the source commit. Stable assets are immutable: replacing a stable binary requires a new patch version and tag.

</details>

## License

This project is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE). You may view, study, modify and use the software for personal and other noncommercial purposes under the license terms; commercial use requires a separate written license from the copyright holder. Earlier versions published under the MIT License remain available under the license granted with those versions.
