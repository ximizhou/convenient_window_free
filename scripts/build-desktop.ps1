param(
  [switch]$RequireTrustedSignature
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "hash-file.ps1")

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
  throw "Desktop packages can only be built on Windows"
}

$repoRoot = Split-Path -Parent $PSScriptRoot
$desktopDir = Join-Path $repoRoot "apps\desktop"
$tauriDir = Join-Path $desktopDir "src-tauri"
$artifactsDir = Join-Path $repoRoot "artifacts"
$portableDir = Join-Path $artifactsDir "ConvenientWindow-portable"
$thirdPartyNotices = Join-Path $repoRoot "target\THIRD-PARTY-NOTICES.txt"
$rootPackage = [System.IO.File]::ReadAllText((Join-Path $repoRoot "package.json"), [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$desktopPackage = [System.IO.File]::ReadAllText((Join-Path $desktopDir "package.json"), [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$tauriConfig = [System.IO.File]::ReadAllText((Join-Path $tauriDir "tauri.conf.json"), [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$version = [string]$rootPackage.version
if (-not $version -or $desktopPackage.version -ne $version -or $tauriConfig.version -ne $version) {
  throw "Desktop version mismatch: root=$($rootPackage.version), frontend=$($desktopPackage.version), tauri=$($tauriConfig.version)"
}
function Get-SourceChanges {
  $unstaged = @(& git -C $repoRoot diff --name-only --)
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect unstaged source changes" }
  $staged = @(& git -C $repoRoot diff --cached --name-only --)
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect staged source changes" }
  $untracked = @(& git -C $repoRoot ls-files --others --exclude-standard)
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect untracked source files" }
  return @($unstaged + $staged + $untracked | Sort-Object -Unique)
}

$sourceCommit = (& git -C $repoRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $sourceCommit -notmatch '^[0-9a-f]{40}$') {
  throw "Unable to read the desktop source commit"
}
$sourceStatus = @(Get-SourceChanges)
$helperToolchain = (Get-Content (Join-Path $repoRoot "rust-toolchain") -Raw).Trim()
$desktopToolchain = "1.96.0-x86_64-pc-windows-msvc"

. (Join-Path $PSScriptRoot "windows-toolchain.ps1")
Initialize-MsvcEnvironment
$buildEnvironmentPath = $env:PATH

& npm --prefix $desktopDir ci
if ($LASTEXITCODE -ne 0) { throw "desktop npm ci failed with exit code $LASTEXITCODE" }
& node (Join-Path $PSScriptRoot "generate-third-party-notices.mjs") $thirdPartyNotices
if ($LASTEXITCODE -ne 0) { throw "third-party notice generation failed with exit code $LASTEXITCODE" }
& (Join-Path $PSScriptRoot "prepare-desktop-sidecar.ps1")
if ($LASTEXITCODE -ne 0) { throw "sidecar preparation failed with exit code $LASTEXITCODE" }
$env:PATH = $buildEnvironmentPath
Remove-Item Env:RUSTC -ErrorAction SilentlyContinue
Remove-Item Env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER -ErrorAction SilentlyContinue

& npm --prefix $desktopDir test
if ($LASTEXITCODE -ne 0) { throw "desktop tests failed with exit code $LASTEXITCODE" }
& npm --prefix $desktopDir run check
if ($LASTEXITCODE -ne 0) { throw "desktop check failed with exit code $LASTEXITCODE" }
& npm --prefix $desktopDir run build
if ($LASTEXITCODE -ne 0) { throw "desktop frontend build failed with exit code $LASTEXITCODE" }

Push-Location (Join-Path $repoRoot "helper")
try {
  & rustup run $helperToolchain cargo fmt --check
  if ($LASTEXITCODE -ne 0) { throw "helper rustfmt failed with exit code $LASTEXITCODE" }
  & rustup run $helperToolchain cargo test
  if ($LASTEXITCODE -ne 0) { throw "helper tests failed with exit code $LASTEXITCODE" }
} finally {
  Pop-Location
}

Push-Location $tauriDir
try {
  & rustup run $desktopToolchain cargo fmt --check
  if ($LASTEXITCODE -ne 0) { throw "desktop rustfmt failed with exit code $LASTEXITCODE" }
  & rustup run $desktopToolchain cargo test
  if ($LASTEXITCODE -ne 0) { throw "desktop Rust tests failed with exit code $LASTEXITCODE" }
} finally {
  Pop-Location
}

& npm --prefix $desktopDir run tauri:build
if ($LASTEXITCODE -ne 0) { throw "Tauri NSIS build failed with exit code $LASTEXITCODE" }

$releaseDir = Join-Path $tauriDir "target\release"
$appExe = Join-Path $releaseDir "convenient-window.exe"
$nsisInstaller = Get-ChildItem (Join-Path $releaseDir "bundle\nsis") -Filter "*.exe" -File |
  Sort-Object LastWriteTime -Descending |
  Select-Object -First 1
if (-not (Test-Path $appExe)) { throw "Tauri executable is missing: $appExe" }
if (-not $nsisInstaller) { throw "NSIS installer was not produced" }

if (Test-Path $artifactsDir) { Remove-Item -Recurse -Force $artifactsDir }
New-Item -ItemType Directory -Force -Path (Join-Path $portableDir "helper") | Out-Null
Copy-Item -Force $appExe (Join-Path $portableDir "ConvenientWindow.exe")
Copy-Item -Force (Join-Path $repoRoot "LICENSE") (Join-Path $portableDir "LICENSE")
Copy-Item -Force $thirdPartyNotices (Join-Path $portableDir "THIRD-PARTY-NOTICES.txt")
$payloadDir = Join-Path $tauriDir "resources\helper"
Get-ChildItem $payloadDir -File |
  Where-Object { $_.Extension -in ".exe", ".dll" -or $_.Name -eq "payload-manifest.json" } |
  Copy-Item -Destination (Join-Path $portableDir "helper") -Force
$portableReadme = @"
Convenient Window portable package for Windows 11 x64.
Run ConvenientWindow.exe. Application data remains in the current user's Local AppData directory.
Noncommercial use is governed by the included LICENSE file. Commercial use requires separate written permission.
Exit from the tray menu before removing this directory.
"@
[System.IO.File]::WriteAllText(
  (Join-Path $portableDir "README.txt"),
  $portableReadme,
  [System.Text.UTF8Encoding]::new($false)
)

$portableZip = Join-Path $artifactsDir "convenient-window-$version-windows-x64-portable.zip"
Compress-Archive -Path (Join-Path $portableDir "*") -DestinationPath $portableZip -CompressionLevel Optimal
$installerCopy = Join-Path $artifactsDir "convenient-window-$version-windows-x64-setup.exe"
Copy-Item -Force $nsisInstaller.FullName $installerCopy

if ($RequireTrustedSignature) {
  $signatureCommand = Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue
  if (-not $signatureCommand) { throw "Get-AuthenticodeSignature is required when -RequireTrustedSignature is set" }
  $signedBinaries = @(
    (Join-Path $portableDir "ConvenientWindow.exe"),
    (Join-Path $portableDir "helper\magic-corners-helper.exe"),
    $installerCopy
  )
  foreach ($binary in $signedBinaries) {
    $signature = & $signatureCommand $binary
    if (-not $signature -or $signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
      throw "Trusted Authenticode signature required for $binary; status=$($signature.Status)"
    }
  }
}

$deliverables = @($installerCopy, $portableZip) | ForEach-Object {
  $file = Get-Item $_
  [ordered]@{
    name = $file.Name
    bytes = $file.Length
    sha256 = (Get-Sha256 $file.FullName)
  }
}
$finalSourceStatus = @(Get-SourceChanges)
if (($sourceStatus -join "`n") -ne ($finalSourceStatus -join "`n")) {
  throw "Desktop build changed the source worktree"
}

$artifactManifest = [ordered]@{
  schemaVersion = 1
  version = $version
  platform = "windows-x64"
  sourceRepository = "https://github.com/ximizhou/convenient_window_free"
  sourceCommit = $sourceCommit
  dirty = ($finalSourceStatus.Count -gt 0)
  deliverables = @($deliverables)
}
$artifactManifestPath = Join-Path $artifactsDir "artifact-manifest.json"
[System.IO.File]::WriteAllText(
  $artifactManifestPath,
  ($artifactManifest | ConvertTo-Json -Depth 5),
  [System.Text.UTF8Encoding]::new($false)
)

$checksumPath = Join-Path $artifactsDir "SHA256SUMS"
$checksumLines = [string[]]@($deliverables | ForEach-Object { "$($_.sha256)  $($_.name)" })
[System.IO.File]::WriteAllLines(
  $checksumPath,
  $checksumLines,
  [System.Text.UTF8Encoding]::new($false)
)

$deliverables | Format-Table -AutoSize
Write-Output "portable directory: $portableDir"
Write-Output "artifact manifest: $artifactManifestPath"
Write-Output "checksums: $checksumPath"
