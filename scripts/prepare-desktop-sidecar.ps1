param(
  [string]$Toolchain,
  [string]$RustupHome = $env:RUSTUP_HOME,
  [string]$CargoHome = $env:CARGO_HOME,
  [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = Split-Path -Parent $PSScriptRoot
$helperDir = Join-Path $repoRoot "helper"
$desktopDir = Join-Path $repoRoot "apps\desktop"
$payloadDir = Join-Path $desktopDir "src-tauri\resources\helper"
$assetPolicyPath = Join-Path $desktopDir "helper-assets.json"
if (-not $Toolchain) { $Toolchain = (Get-Content (Join-Path $repoRoot "rust-toolchain") -Raw).Trim() }
if (-not $Toolchain) { throw "rust-toolchain is empty" }
$target = "x86_64-pc-windows-gnullvm"

if ($RustupHome) { $env:RUSTUP_HOME = $RustupHome }
if ($CargoHome) { $env:CARGO_HOME = $CargoHome }
$rustcExe = (& rustup which --toolchain $Toolchain rustc).Trim()
if ($LASTEXITCODE -ne 0 -or -not (Test-Path $rustcExe)) {
  throw "Rust toolchain is unavailable: $Toolchain"
}
$toolchainBin = Split-Path -Parent $rustcExe
$cargoExe = Join-Path $toolchainBin "cargo.exe"
$targetBin = Join-Path $toolchainBin "..\lib\rustlib\$target\bin"
$targetBin = [System.IO.Path]::GetFullPath($targetBin)
if (-not (Test-Path $cargoExe)) { throw "cargo.exe is unavailable in $toolchainBin" }

$env:RUSTC = $rustcExe
$env:PATH = "$toolchainBin;$targetBin;$env:PATH"
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER = Join-Path $targetBin "rust-lld.exe"
if (-not (Test-Path $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER)) {
  throw "Bundled Rust linker is unavailable: $env:CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER"
}

if (-not $SkipBuild) {
  Push-Location $helperDir
  try {
    & $cargoExe build --release
    if ($LASTEXITCODE -ne 0) { throw "helper cargo build failed with exit code $LASTEXITCODE" }
  } finally {
    Pop-Location
  }
}

$releaseDir = Join-Path $helperDir "target\$target\release"
$helperExe = Join-Path $releaseDir "magic-corners-helper.exe"
$unwindDll = Join-Path $toolchainBin "libunwind.dll"
$stdDlls = @(Get-ChildItem -Path $toolchainBin -Filter "std-*.dll" -File)
if (-not (Test-Path $helperExe)) { throw "Built helper is missing: $helperExe" }
if (-not (Test-Path $unwindDll)) { throw "GNU runtime is missing: $unwindDll" }
if ($stdDlls.Count -eq 0) { throw "GNU runtime is incomplete: no std-*.dll in $toolchainBin" }

New-Item -ItemType Directory -Force -Path $payloadDir | Out-Null
Get-ChildItem -Path $payloadDir -File -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -ne ".gitkeep" } |
  Remove-Item -Force
Copy-Item -Force $helperExe $payloadDir
Copy-Item -Force $unwindDll $payloadDir
$stdDlls | ForEach-Object { Copy-Item -Force $_.FullName $payloadDir }

$policy = Get-Content $assetPolicyPath -Raw | ConvertFrom-Json
if ($policy.version -ne "0.5.5" -or $policy.platform -ne "win32-x64") {
  throw "helper-assets.json does not declare helper 0.5.5 for win32-x64"
}
$payloadFiles = @(Get-ChildItem -Path $payloadDir -File |
  Where-Object { $_.Extension -in ".exe", ".dll" } |
  Sort-Object Name)
$expectedCount = 2 + $stdDlls.Count
if ($payloadFiles.Count -ne $expectedCount) {
  throw "Unexpected sidecar payload count: expected $expectedCount, found $($payloadFiles.Count)"
}

$manifestFiles = @($payloadFiles | ForEach-Object {
  [ordered]@{
    name = $_.Name
    bytes = $_.Length
    sha256 = (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant()
  }
})
$manifest = [ordered]@{
  schemaVersion = 1
  helperVersion = $policy.version
  target = $target
  files = $manifestFiles
}
$manifestPath = Join-Path $payloadDir "payload-manifest.json"
$manifestJson = $manifest | ConvertTo-Json -Depth 5
[System.IO.File]::WriteAllText($manifestPath, $manifestJson, [System.Text.UTF8Encoding]::new($false))

$payloadFiles | Select-Object Name, Length, @{ Name = "SHA256"; Expression = { (Get-FileHash -Algorithm SHA256 $_.FullName).Hash.ToLowerInvariant() } }
Write-Output "sidecar manifest: $manifestPath"
