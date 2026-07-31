# Convenient Window Free

Open-source Windows 11 desktop software for hot zones, window edge hiding, global mouse gestures, screenshots, local OCR, and topmost-window controls.

This repository owns two things:

- `apps/desktop/`: the Tauri 2 desktop host and reusable Svelte interface.
- `helper/`: the only source tree for the shared Rust helper used by both the desktop app and the private uTools integration.

The repository is being initialized from a verified uTools baseline. Until the first standalone build is checked in, the architecture and acceptance boundary are documented in [`docs/architecture.md`](docs/architecture.md) and [`docs/testing.md`](docs/testing.md). Do not infer release availability from repository visibility.

## Support

The first accepted target is Windows 11 x64. Linux and macOS have reserved platform boundaries only; they are not implemented or supported.

## Development Modes

Standalone desktop development happens entirely in this repository. The private uTools project mounts this repository at `open-source/` and builds the same helper source from `open-source/helper/`; it does not keep another helper source copy.

Planned top-level build entry points are PowerShell and npm commands that build the Svelte frontend, Rust helper sidecar, Tauri application, per-user NSIS installer, and portable archive. Exact commands are added alongside the implementation and must remain executable in Windows CI.

## Security

Local WebSocket access is protected by a random token stored in each product's own data directory. The helper uses a global Windows single-instance lock so the uTools and standalone hosts cannot control competing helper processes. See [`SECURITY.md`](SECURITY.md) for private vulnerability reporting.

## License

MIT. See [`LICENSE`](LICENSE).
