<div align="center">

<img src="docs/images/app-icon.png" width="72" alt="Convenient Window icon">

<h1>Convenient Window</h1>

<p><strong>Tuck windows away. Keep everyday actions within reach.</strong></p>
<p>Edge hiding, hot corners, mouse gestures and saved layouts for your Windows desktop.</p>

<p>
  <a href="https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.11"><img src="https://img.shields.io/badge/Desktop-0.6.11-3f6ee4?style=flat-square" alt="Desktop stable 0.6.11"></a>
  <img src="https://img.shields.io/badge/Windows_11-x64-59636e?style=flat-square" alt="Windows 11 x64">
</p>

<p>
  <a href="https://github.com/ximizhou/convenient_window_free/releases/download/v0.6.11/convenient-window-0.6.11-windows-x64-setup.exe"><strong>Download for Windows</strong></a> ·
  <a href="https://github.com/ximizhou/convenient_window_free/releases/download/v0.6.11/convenient-window-0.6.11-windows-x64-portable.zip">Portable ZIP</a> ·
  <a href="docs/release-notes/0.6.11.md">Release notes</a>
</p>

<p><a href="README.zh-CN.md">简体中文</a> · <strong>English</strong></p>

</div>

![Convenient Window desktop: light settings panel with multiple displays](docs/images/running-center.png)

<p align="center"><sub>Standalone app · System tray access · Chinese and English · Light and dark themes</sub></p>

## Fewer repetitive steps

| Feature | What it does |
| --- | --- |
| **Edge hiding** | Tuck a window against a screen edge, then hover over its strip to bring it back. |
| **Hot corners and zones** | Assign actions to each display's corners and edges: show the desktop, adjust volume or send shortcuts. |
| **Mouse gestures** | Draw a gesture to run window controls, shortcuts or custom actions. |
| **Window layouts** | Save window positions, sizes and states, then restore the arrangement. |
| **Move, resize and pin** | Drag and resize from anywhere in a window, or keep a reference window on top. |
| **Screenshots, pinned images and OCR** | Capture an area, float it above other windows and extract text with local Windows OCR. |

## A closer look at the desktop app

### Set up each display

Choose a display, assign corner and edge actions, and adjust the size and behavior of its hot zones.

![Desktop hot-corner and zone settings](docs/images/hot-zones.png)

### Keep your window arrangements

Save a layout and restore windows that are still running. Closed windows and windows without sufficient access are skipped.

![Desktop window layouts](docs/images/window-layouts.png)

<details>
<summary>See mouse gesture settings</summary>

![Desktop mouse gesture settings](docs/images/mouse-gestures.png)

</details>

*Screenshots show the Chinese interface. The app also supports English.*

## Get started

1. Download the **Windows installer** above, or extract and run the portable edition.
2. Open Convenient Window from the system tray, turn on the main switch and enable the features you want.
3. Try edge hiding: drag an ordinary, non-maximized window to an enabled edge and release when the blue hint appears. At the top edge, wait for the line to fill. Hover over the strip to restore the window.

Closing settings leaves the app running in the tray; use the tray menu to quit. The helper is included, with no separate download required.

<details>
<summary>Compatibility, updates and usage notes</summary>

- The published and tested desktop edition targets **Windows 11 x64**. This release has no accepted macOS/Linux installation packages.
- Window operations, settings and OCR stay on your machine. OCR does not upload screenshots and uses installed Windows language packs.
- Download updates manually. For the portable edition, replace the program files; settings remain in the local application data directory.
- Executables are currently unsigned, so Windows may show an unknown-publisher prompt. The [release page](https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.11) includes checksums and a manifest to verify the download.
- Taskbar material is experimental and off by default; appearance and restoration need verification on the target system.

[Previous releases](https://github.com/ximizhou/convenient_window_free/releases) · [0.6.10 rollback](https://github.com/ximizhou/convenient_window_free/releases/tag/v0.6.10)

</details>

## Also available in DSH

Already using DeepSeek Harness? Open Convenient Window from its sidebar with the same settings UI. The enabled helper keeps running when you switch tabs or minimize DSH.

![Convenient Window in the DSH sidebar](docs/images/dsh-panel.png)

**DSH plugin 0.2.0-beta.3** · Tested with **DSH 0.2.2-rc.5** · Windows 11 x64

[Install and upgrade (Chinese)](apps/dsh-plugin/README.md#安装) · [Plugin download](https://github.com/ximizhou/convenient_window_free/releases/tag/dsh-v0.2.0-beta.3)

The helper is bundled. Desktop and DSH settings are stored separately; enable only one host's window helper at a time. Other DSH versions need separate verification.

## Feedback and development

When [reporting a problem](https://github.com/ximizhou/convenient_window_free/issues/new/choose), include the edition, DSH/plugin/Windows versions, and steps to reproduce. Report security issues privately as described in [SECURITY.md](SECURITY.md).

<details>
<summary><strong>Source layout, builds and verification</strong></summary>

- `apps/desktop/`: standalone host and shared Svelte settings UI.
- `apps/dsh-plugin/`: DSH integration, native adapter, packaging and regression tests.
- `helper/`: the single Rust window engine source.
- `scripts/`: desktop build, audit and release tooling.

Desktop builds use Node.js 24.14.0 and the repository's pinned Rust toolchains. Windows native builds require Visual Studio C++ Build Tools and a Windows SDK.

```powershell
npm ci
npm run desktop:install
npm run desktop:test
npm run desktop:check
npm run desktop:build
```

[Architecture](docs/architecture.md) · [Testing](docs/testing.md) · [Desktop releases](docs/release.md) · [DSH builds and releases (Chinese)](apps/dsh-plugin/README.md#维护者)

Desktop stable assets remain immutable. npm and GitHub distribute the same accepted DSH package. Documentation-only changes do not replace published packages.

</details>

## License

Source is available under [PolyForm Noncommercial 1.0.0](LICENSE); see the license for its terms. Earlier versions released under MIT retain their original license.
