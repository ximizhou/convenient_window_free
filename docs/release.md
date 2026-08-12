# Release Process

The standalone desktop uses its final version number for acceptance. A fixed `main` commit is built once, published as a GitHub Pre-release, downloaded and tested through the public path, and then promoted to a stable release without replacing any asset.

## Prepare

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

`desktop:build` records the source commit and dirty state in `artifacts/artifact-manifest.json`. A release candidate is invalid unless `dirty` is `false`, the source commit equals `main`, and `SHA256SUMS` matches the installer and portable archive.

## Publish And Accept

Create the public Pre-release:

```powershell
npm run desktop:publish:pre
```

The command refuses to overwrite an existing tag, uploads only the NSIS installer, portable ZIP, manifest, and checksums, then anonymously downloads every asset and recomputes its SHA-256. Test the downloaded installer, uninstall flow, and portable ZIP on Windows 11 x64. The current binaries are unsigned, so an unknown-publisher or SmartScreen warning is expected and must not be described as a trusted signature.

If acceptance succeeds, promote the existing release in place:

```powershell
npm run desktop:release:promote
```

Promotion verifies the remote asset set and hashes again, then changes only the GitHub release state from Pre-release to stable. It does not rebuild or upload files. If acceptance fails or any asset must change, keep the failed version immutable, increase the patch version, and repeat the process with a new tag.
