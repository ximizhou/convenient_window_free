# Convenient Window Free

Source-available Windows 11 desktop software for hot zones, window edge hiding, global mouse gestures, screenshots, local OCR, and topmost-window controls.

Settings persist and apply as they change unless an operation explicitly requires confirmation. Window enhancement currently provides one keyboard-accessible, CSS-only tutorial beside edge hiding; drag/resize and topmost-pin settings intentionally have no tutorial entry.

This repository owns two things:

- `apps/desktop/`: the Tauri 2 desktop host and reusable Svelte interface.
- `helper/`: the only source tree for the shared Rust helper used by both the desktop app and the private uTools integration.

This repository contains the shared helper, the standalone Tauri host, their automated tests, and reproducible Windows packaging scripts. A local build can produce an NSIS installer and portable archive; repository visibility still does not imply that a public release is available.

## Support

The first accepted target is Windows 11 x64. Linux and macOS have reserved platform boundaries only; they are not implemented or supported.

## Development Modes

Standalone desktop development happens entirely in this repository. The private uTools project mounts this repository at `open-source/` and builds the same helper source from `open-source/helper/`; it does not keep another helper source copy.

The top-level build entry point builds the Svelte frontend, Rust helper sidecar, Tauri application, per-user NSIS installer, and portable archive:

```powershell
npm run desktop:build
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
npm run desktop:runtime-force-kill-smoke
npm run desktop:install-smoke
npm run desktop:audit
```

The build derives a deterministic `THIRD-PARTY-NOTICES.txt` from the installed npm production tree and the locked Windows Cargo dependency graphs, then packages it with the project `LICENSE` in both NSIS and portable outputs. Missing or unauditable dependency license text stops the build.

The three portable runtime smoke commands use explicit disposable data roots and verify normal shutdown, global-helper conflict handling, and Job Object cleanup after the desktop process is force-terminated without modifying the real user profile. The NSIS smoke installs under a disposable directory and uninstalls while the app and its helper are running; it requires graceful helper shutdown, then repeats the uninstall with a separately owned helper to prove that desktop cleanup does not terminate the uTools-owned process. Registry, shortcut, port, process, and directory cleanup are verified. These commands must remain executable in Windows CI.

Public downloads use the immutable Pre-release acceptance flow documented in [`docs/release.md`](docs/release.md): build once from a clean `main`, download and test those exact assets, then promote the same release without replacing files.

## Helper Development

Install the pinned Rust toolchain, then run commands from `helper/` so its linker configuration is applied:

```powershell
rustup toolchain install 1.96.0-x86_64-pc-windows-gnullvm --profile minimal --component rustfmt
cd helper
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo fmt --check
rustup run 1.96.0-x86_64-pc-windows-gnullvm cargo test
```

The helper accepts an absolute `--data-dir`. Protocol v6 emits generic `host.action` events; the old `utools-redirect` configuration value remains an input alias for schema v7 compatibility. Use `scripts/helper-instance-smoke.mjs <packaged-helper.exe>` to verify global lock contention and recovery with two isolated data directories.

## Security

Local WebSocket access is protected by a random token stored in each product's own data directory. The helper uses a global Windows single-instance lock so the uTools and standalone hosts cannot control competing helper processes. See [`SECURITY.md`](SECURITY.md) for private vulnerability reporting.

## License

This project is source-available under the [PolyForm Noncommercial License 1.0.0](LICENSE).

You may view, study, modify, and use the software for personal and other noncommercial purposes under the license terms. Commercial use requires a separate written license from the copyright holder.

本项目允许在许可证条款下查看、学习、修改，以及用于个人和其他非商业目的。任何商业使用均须事先取得版权所有者的书面授权。

This license applies to the repository from the commit that introduced it onward. Earlier versions that were already published under the MIT License remain available under the license granted with those versions.
