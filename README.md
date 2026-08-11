# Convenient Window Free

Open-source Windows 11 desktop software for hot zones, window edge hiding, global mouse gestures, screenshots, local OCR, and topmost-window controls.

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
npm run desktop:install-smoke
npm run desktop:audit
```

The two portable runtime smoke commands use explicit disposable data roots and verify that the real user profile is not modified. The NSIS smoke installs under a disposable directory, launches the installed app with isolated data, then verifies uninstaller, registry, shortcut, and directory cleanup. These commands must remain executable in Windows CI.

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

MIT. See [`LICENSE`](LICENSE).
