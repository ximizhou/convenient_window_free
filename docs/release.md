# Release Process

The standalone desktop has two user acceptance points:

1. During daily `develop` work, acceptance normally stays in the host integration for faster iteration. Only when the user explicitly requests desktop synchronization, port the applicable shared behavior, preserve desktop-specific lifecycle and UI differences, build the current NSIS package, and give the user its absolute local path. Do not publish this build.
2. Before release, freeze a clean `main`, build once with the final version, publish those exact assets as a GitHub Pre-release, and have the user download and test the public installer and portable archive. Promote the same release without replacing assets.

## Develop Acceptance

Desktop packaging is not repeated for every uTools change. When desktop synchronization is requested, run `npm run desktop:build` and the relevant lifecycle gates, then hand off the generated `artifacts/convenient-window-<version>-windows-x64-setup.exe` with its absolute path, source commit, size, and SHA-256. This first pass is for fast product feedback; it is not release evidence and does not consume a tag.

## Prepare Final Online Acceptance

1. Merge the accepted source into `main` and push it.
2. Confirm `package.json`, `apps/desktop/package.json`, and `apps/desktop/src-tauri/tauri.conf.json` declare the same version.
3. From a clean `main` that exactly matches `origin/main`, run the complete package and lifecycle gates:

```powershell
npm run desktop:build
node scripts/helper-instance-smoke.mjs apps/desktop/src-tauri/resources/helper/magic-corners-helper.exe
npm run desktop:runtime-smoke
npm run desktop:runtime-conflict-smoke
npm run desktop:runtime-force-kill-smoke
npm run desktop:install-smoke
npm run desktop:audit
```

`desktop:build` records the source commit and dirty state in `artifacts/artifact-manifest.json`. It also generates `THIRD-PARTY-NOTICES.txt` from the installed npm production tree and the locked Windows Cargo dependency graphs; missing or unauditable license text stops the build. A release candidate is invalid unless `dirty` is `false`, the source commit equals `main`, `SHA256SUMS` matches the installer and portable archive, and both package types contain the project `LICENSE` plus the generated third-party notices.

## Publish And Accept

Create the public Pre-release:

```powershell
npm run desktop:publish:pre
```

The command refuses to overwrite an existing tag, uploads only the NSIS installer, portable ZIP, manifest, and checksums, then anonymously downloads every asset and recomputes its SHA-256. Public asset names must remain ASCII so Windows PowerShell 5.1 and the GitHub API compare the same exact names. Test the downloaded installer, uninstall flow, and portable ZIP on Windows 11 x64. The current binaries are unsigned, so an unknown-publisher or SmartScreen warning is expected and must not be described as a trusted signature.

If acceptance succeeds, promote the existing release in place:

```powershell
npm run desktop:release:promote
```

Promotion verifies the remote asset set and hashes again, then changes only the GitHub release state from Pre-release to stable. It does not rebuild or upload files. If acceptance fails or any asset must change, keep the failed version immutable, increase the patch version, and repeat the process with a new tag.
