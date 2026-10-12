# Release Process

Version 0.6.4 is the stable Windows x64 baseline as of October 11, 2026; it was promoted without rebuilding or replacing assets. Development source and the local desktop helper policy are now 0.6.5 for the first window-motion task. Stable downloads, tags and accepted 0.6.4 artifacts keep their existing identity; no 0.6.5 installer or online assets are asserted.

The standalone desktop has two user acceptance points:

1. During daily `develop` work, acceptance normally stays in the host integration for faster iteration. Only when the user explicitly requests desktop synchronization, port the applicable shared behavior, preserve desktop-specific lifecycle and UI differences, build the current NSIS package, and give the user its absolute local path. Do not publish this build.
2. Before release, freeze a clean `main`, build the final version, publish those exact assets as a GitHub Pre-release, and have the user download and test the public installer and portable archive. While it remains a Pre-release, an explicitly requested same-version candidate refresh may replace the complete asset set; after promotion, the release is immutable.

## Develop Acceptance

Desktop packaging is not repeated for every host-integration change. When desktop synchronization is requested, build the current package and relevant lifecycle gates, then hand off the exact installer with its version, source commit, size and SHA-256. Local acceptance is not release evidence and does not consume a tag.

For the 0.6.4 candidate, preserve existing 0.6.3 local artifacts:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-desktop.ps1 -ArtifactsDir artifacts/0.6.4
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/audit-desktop-artifacts.ps1 -ArtifactsDir artifacts/0.6.4
```

`-ArtifactsDir` is restricted to this repository's `artifacts/` tree. The expected installer is `artifacts/0.6.4/convenient-window-0.6.4-windows-x64-setup.exe`; its manifest, checksums and portable output belong in the same selected directory. Runtime and install gates must explicitly select that output; default npm scripts still target root artifacts. See [candidate acceptance](testing.md#064-local-candidate-acceptance). An expected path or an older package's result does not prove a current build.

Development source and the desktop local helper policy use 0.6.5. An integration retains its immutable published 0.6.4 download allow-list while exercising the newly built local helper; that does not create 0.6.5 download assets. `scripts/version-audit.ps1 -DevelopmentOnly -HelperAssetsPath <published-helper-assets.json>` excludes only that explicitly supplied published manifest from source-version equality and labels it as such. This is a development source check, not release freezing or a hash/asset acceptance check. Without `-DevelopmentOnly`, the default strict version comparison is unchanged. Never pass the development switch to the final release-freeze gate or fabricate new URLs/hashes before publication.

An existing 0.6.3 installation must remain intact during automatic local preparation: do not run NSIS replacement/uninstall against it. The user performs the first-stage installation/upgrade; automatic installer gates run only in isolated accounts/CI without existing product registrations or processes.

0.6.3 stable assets remain immutable. This 0.6.4 local phase publishes nothing and does not advance the stable branch. The [draft notes](release-notes/0.6.4.md) describe the candidate; automated results and artifact identities must be added only after verification. UAC, mixed-DPI/multi-monitor behavior and taskbar restoration still require real-machine acceptance.

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

The command uploads only the NSIS installer, portable ZIP, manifest, and checksums, then anonymously downloads every asset and recomputes its SHA-256. For an active Pre-release, use the explicit `-ReplacePreRelease` path to replace the complete same-version candidate; the command refuses to replace a stable release or an unknown state. Public asset names must remain ASCII so Windows PowerShell 5.1 and the GitHub API compare the same exact names. Test the downloaded installer, uninstall flow, and portable ZIP on Windows 11 x64. The current binaries are unsigned, so an unknown-publisher or SmartScreen warning is expected and must not be described as a trusted signature.

If acceptance succeeds, promote the existing release in place:

```powershell
npm run desktop:release:promote
```

Promotion verifies the remote asset set and hashes again, then changes only the GitHub release state from Pre-release to stable. It does not rebuild or upload files. If acceptance fails while the release is still a Pre-release, refresh the same-version candidate with `-ReplacePreRelease`, then make users download it again. Once promoted, the release and its assets are immutable; any later binary change requires a new patch version and tag.
