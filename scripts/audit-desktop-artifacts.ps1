param(
  [string]$ArtifactsDir,
  [string]$SevenZip = $env:SEVEN_ZIP
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "hash-file.ps1")

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $ArtifactsDir) { $ArtifactsDir = Join-Path $repoRoot "artifacts" }
$manifestPath = Join-Path $ArtifactsDir "artifact-manifest.json"
if (-not (Test-Path $manifestPath)) { throw "Artifact manifest is missing: $manifestPath" }
$artifactManifest = [System.IO.File]::ReadAllText($manifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$rootPackage = [System.IO.File]::ReadAllText((Join-Path $repoRoot "package.json"), [System.Text.Encoding]::UTF8) | ConvertFrom-Json
if ($artifactManifest.version -ne $rootPackage.version -or $artifactManifest.platform -ne "windows-x64") {
  throw "Artifact manifest declares an unexpected version or platform"
}
if ($artifactManifest.sourceRepository -ne "https://github.com/ximizhou/convenient_window_free" -or
    $artifactManifest.sourceCommit -notmatch '^[0-9a-f]{40}$') {
  throw "Artifact manifest declares an invalid source"
}
if ($artifactManifest.dirty) { throw "Artifact manifest was generated from a dirty source worktree" }
$head = (& git -C $repoRoot rev-parse HEAD).Trim()
if ($LASTEXITCODE -ne 0 -or $head -ne $artifactManifest.sourceCommit) {
  throw "Artifact manifest sourceCommit does not match the current source commit"
}
if (@($artifactManifest.deliverables).Count -ne 2) {
  throw "Artifact manifest must declare exactly the NSIS installer and portable archive"
}

foreach ($declared in $artifactManifest.deliverables) {
  $path = Join-Path $ArtifactsDir $declared.name
  if (-not (Test-Path $path -PathType Leaf)) { throw "Declared artifact is missing: $path" }
  $file = Get-Item $path
  $hash = (Get-Sha256 $path)
  if ($file.Length -ne [long]$declared.bytes) { throw "Artifact size mismatch: $($file.Name)" }
  if ($hash -ne $declared.sha256) { throw "Artifact hash mismatch: $($file.Name)" }
}
$checksumPath = Join-Path $ArtifactsDir "SHA256SUMS"
if (-not (Test-Path $checksumPath -PathType Leaf)) { throw "SHA256SUMS is missing" }
$expectedChecksums = @($artifactManifest.deliverables | ForEach-Object { "$($_.sha256)  $($_.name)" })
$actualChecksums = @([System.IO.File]::ReadAllLines($checksumPath, [System.Text.Encoding]::UTF8))
if (($actualChecksums -join "`n") -ne ($expectedChecksums -join "`n")) {
  throw "SHA256SUMS does not match the artifact manifest"
}

function Assert-ExactFiles {
  param([string]$Root, [string[]]$Expected)
  $actual = @(Get-ChildItem $Root -Recurse -File | ForEach-Object {
    $_.FullName.Substring($Root.Length).TrimStart("\").Replace("\", "/")
  } | Sort-Object)
  $expectedSorted = @($Expected | Sort-Object)
  if (($actual -join "`n") -ne ($expectedSorted -join "`n")) {
    throw "Unexpected package inventory in $Root`nExpected:`n$($expectedSorted -join "`n")`nActual:`n$($actual -join "`n")"
  }
}

$expectedThirdPartyNotices = Join-Path $repoRoot "target\THIRD-PARTY-NOTICES.txt"
& node (Join-Path $PSScriptRoot "generate-third-party-notices.mjs") $expectedThirdPartyNotices
if ($LASTEXITCODE -ne 0) { throw "Unable to regenerate third-party notices for artifact audit" }
$expectedThirdPartyHash = Get-Sha256 $expectedThirdPartyNotices

function Assert-ThirdPartyNotices {
  param([string]$Path)
  if (-not (Test-Path $Path -PathType Leaf)) { throw "Third-party notices are missing: $Path" }
  $text = [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
  $required = @(
    "THIRD-PARTY COMPONENTS",
    "https://github.com/ximizhou/convenient_window_free",
    "npm:@tauri-apps/api@",
    "cargo:tauri@",
    "Mozilla Public License Version 2.0",
    "Apache License",
    "Permission is hereby granted, free of charge",
    "Redistribution and use in source and binary forms"
  )
  foreach ($marker in $required) {
    if (-not $text.Contains($marker)) { throw "Third-party notices are incomplete; missing '$marker': $Path" }
  }
  if ($text.Contains("PolyForm Noncommercial License 1.0.0")) {
    throw "Project license must not be duplicated in third-party notices: $Path"
  }
  if ((Get-Sha256 $Path) -ne $expectedThirdPartyHash) {
    throw "Third-party notices do not match the current locked dependency graph: $Path"
  }
}

function Assert-HelperPayload {
  param([string]$HelperDir)
  $payloadManifestPath = Join-Path $HelperDir "payload-manifest.json"
  if (-not (Test-Path $payloadManifestPath)) { throw "Sidecar payload manifest is missing" }
  $payloadManifest = [System.IO.File]::ReadAllText($payloadManifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
  if ($payloadManifest.helperVersion -ne "0.5.5" -or $payloadManifest.target -ne "x86_64-pc-windows-gnullvm") {
    throw "Sidecar payload manifest declares an unexpected helper"
  }
  $expected = @("payload-manifest.json") + @($payloadManifest.files | ForEach-Object { $_.name })
  Assert-ExactFiles -Root $HelperDir -Expected $expected
  foreach ($declared in $payloadManifest.files) {
    $path = Join-Path $HelperDir $declared.name
    $file = Get-Item $path
    if ($file.Length -ne [long]$declared.bytes) { throw "Sidecar size mismatch: $($declared.name)" }
    $hash = (Get-Sha256 $path)
    if ($hash -ne $declared.sha256) { throw "Sidecar hash mismatch: $($declared.name)" }
  }
  if (-not (Test-Path (Join-Path $HelperDir "magic-corners-helper.exe"))) { throw "Sidecar EXE is missing" }
  if (-not (Test-Path (Join-Path $HelperDir "libunwind.dll"))) { throw "libunwind.dll is missing" }
  if (@(Get-ChildItem $HelperDir -Filter "std-*.dll" -File).Count -eq 0) { throw "std-*.dll is missing" }
}

$portableDir = Join-Path $ArtifactsDir "ConvenientWindow-portable"
$portableManifestPath = Join-Path $portableDir "helper\payload-manifest.json"
$portableManifest = [System.IO.File]::ReadAllText($portableManifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
$portableFiles = @(
  "ConvenientWindow.exe",
  "LICENSE",
  "THIRD-PARTY-NOTICES.txt",
  "README.txt",
  "helper/payload-manifest.json"
) + @($portableManifest.files | ForEach-Object { "helper/$($_.name)" })
Assert-ExactFiles -Root $portableDir -Expected $portableFiles
Assert-ThirdPartyNotices -Path (Join-Path $portableDir "THIRD-PARTY-NOTICES.txt")
Assert-HelperPayload -HelperDir (Join-Path $portableDir "helper")

$portableZip = Get-ChildItem $ArtifactsDir -Filter "*-portable.zip" -File | Select-Object -First 1
if (-not $portableZip) { throw "Portable zip is missing" }
$tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("convenient-window-audit-" + [guid]::NewGuid().ToString("N"))
$zipRoot = Join-Path $tempRoot "zip"
$nsisRoot = Join-Path $tempRoot "nsis"
New-Item -ItemType Directory -Force -Path $zipRoot, $nsisRoot | Out-Null
try {
  Expand-Archive -Path $portableZip.FullName -DestinationPath $zipRoot
  Assert-ExactFiles -Root $zipRoot -Expected $portableFiles
  Assert-ThirdPartyNotices -Path (Join-Path $zipRoot "THIRD-PARTY-NOTICES.txt")
  Assert-HelperPayload -HelperDir (Join-Path $zipRoot "helper")

  if (-not $SevenZip) {
    $sevenZipCandidates = @(
      (Get-Command 7z.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue),
      (Join-Path $env:ProgramFiles "7-Zip\7z.exe")
    ) | Where-Object { $_ -and (Test-Path $_) }
    $SevenZip = $sevenZipCandidates | Select-Object -First 1
  }
  if (-not $SevenZip -or -not (Test-Path $SevenZip)) {
    throw "7z.exe is required to inspect the NSIS payload; set SEVEN_ZIP or -SevenZip"
  }
  $installer = Get-ChildItem $ArtifactsDir -Filter "*setup.exe" -File | Select-Object -First 1
  if (-not $installer) { throw "NSIS installer is missing" }
  & $SevenZip x -y "-o$nsisRoot" $installer.FullName | Out-Null
  if ($LASTEXITCODE -ne 0) { throw "7-Zip could not extract the NSIS installer" }

  $nsisManifestPath = Join-Path $nsisRoot "helper\payload-manifest.json"
  $nsisManifest = [System.IO.File]::ReadAllText($nsisManifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
  $nsisFiles = @(
    '$PLUGINSDIR/System.dll',
    '$PLUGINSDIR/modern-wizard.bmp',
    '$PLUGINSDIR/nsDialogs.dll',
    '$PLUGINSDIR/nsis_tauri_utils.dll',
    '$PLUGINSDIR/StartMenu.dll',
    '$PLUGINSDIR/NSISdl.dll',
    "convenient-window.exe",
    "LICENSE",
    "THIRD-PARTY-NOTICES.txt",
    "helper/payload-manifest.json"
  ) + @($nsisManifest.files | ForEach-Object { "helper/$($_.name)" })
  Assert-ExactFiles -Root $nsisRoot -Expected $nsisFiles
  Assert-ThirdPartyNotices -Path (Join-Path $nsisRoot "THIRD-PARTY-NOTICES.txt")
  Assert-HelperPayload -HelperDir (Join-Path $nsisRoot "helper")

  foreach ($binary in @(
    (Join-Path $portableDir "ConvenientWindow.exe"),
    $installer.FullName
  )) {
    $signature = Get-AuthenticodeSignature $binary
    if ($signature.Status -ne [System.Management.Automation.SignatureStatus]::NotSigned) {
      throw "Unexpected code-signing state for $binary`: $($signature.Status)"
    }
  }

  $forbiddenNames = '(?i)(^|[\\/])(?:node_modules|target|\.git)([\\/]|$)|auth-token|config\.json|\.log$|\.env$|PROGRESS\.md|BLOCKED\.md'
  $packageFiles = @(Get-ChildItem $portableDir, $zipRoot, $nsisRoot -Recurse -File)
  $forbidden = @($packageFiles | Where-Object { $_.FullName -match $forbiddenNames })
  if ($forbidden.Count -gt 0) {
    throw "Forbidden files found in package: $($forbidden.FullName -join ', ')"
  }

  Push-Location $repoRoot
  try {
    $trackedForbidden = @(git ls-files | Select-String -Pattern '(^|/)(node_modules|target|artifacts)/|(^|/)(auth-token|config\.json)$|\.log$|\.env$')
    if ($LASTEXITCODE -ne 0) { throw "git ls-files failed while auditing tracked source" }
    if ($trackedForbidden.Count -gt 0) { throw "Forbidden generated files are tracked: $trackedForbidden" }

    $sourcePaths = @(git ls-files --cached --others --exclude-standard)
    if ($LASTEXITCODE -ne 0) { throw "git ls-files failed while enumerating public source" }
    if ($sourcePaths.Count -eq 0) { throw "No public source files were found for credential scanning" }
    $secretPattern = 'github_pat_[A-Za-z0-9_]+|ghp_[A-Za-z0-9]+|BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY'
    $privateRemotePattern = 'https://github\.com/ximizhou/convenient_window(?:\.git)?(?:[\s"''`]|$)'
    $sourceMatches = @()
    foreach ($relativePath in $sourcePaths) {
      $fullPath = Join-Path $repoRoot $relativePath
      if (-not (Test-Path $fullPath -PathType Leaf)) { continue }
      $content = [System.Text.Encoding]::UTF8.GetString([System.IO.File]::ReadAllBytes($fullPath))
      if ($content -match $secretPattern) {
        $sourceMatches += "$relativePath`: potential credential"
      }
      if ($content -match $privateRemotePattern) {
        $sourceMatches += "$relativePath`: private repository URL"
      }
    }
    if ($sourceMatches.Count -gt 0) {
      throw "Sensitive material found in public source: $($sourceMatches -join ', ')"
    }
  } finally {
    Pop-Location
  }

  Write-Output "artifact audit: passed"
  Write-Output "deliverables verified: $($artifactManifest.deliverables.Count)"
  Write-Output "portable files verified: $($portableFiles.Count)"
  Write-Output "NSIS files verified: $($nsisFiles.Count)"
  Write-Output "public source files scanned: $($sourcePaths.Count)"
  Write-Output "signing state: NotSigned (expected)"
} finally {
  Remove-Item -Recurse -Force $tempRoot -ErrorAction SilentlyContinue
}
