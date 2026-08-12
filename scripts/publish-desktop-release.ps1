param(
  [string]$Repository = "ximizhou/convenient_window_free",
  [switch]$Promote,
  [switch]$DryRun
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
. (Join-Path $PSScriptRoot "hash-file.ps1")

$repoRoot = Split-Path -Parent $PSScriptRoot
$artifactsDir = Join-Path $repoRoot "artifacts"
$manifestPath = Join-Path $artifactsDir "artifact-manifest.json"
$checksumPath = Join-Path $artifactsDir "SHA256SUMS"
if (-not (Test-Path $manifestPath -PathType Leaf)) { throw "Artifact manifest is missing: $manifestPath" }
if (-not (Test-Path $checksumPath -PathType Leaf)) { throw "SHA256SUMS is missing: $checksumPath" }

function Assert-RemoteAssets {
  param(
    [Parameter(Mandatory = $true)]$Release,
    [Parameter(Mandatory = $true)][System.IO.FileInfo[]]$ExpectedFiles
  )

  $remoteAssets = @($Release.assets | Sort-Object name)
  if (($remoteAssets.name -join "`n") -ne (($ExpectedFiles.Name | Sort-Object) -join "`n")) {
    throw "Remote release asset set does not match the local candidate"
  }
  $tempRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("convenient-window-release-" + [guid]::NewGuid().ToString("N"))
  New-Item -ItemType Directory -Force -Path $tempRoot | Out-Null
  try {
    foreach ($remote in $remoteAssets) {
      $expected = $ExpectedFiles | Where-Object Name -eq $remote.name | Select-Object -First 1
      if (-not $expected -or [long]$remote.size -ne $expected.Length) {
        throw "Remote release asset size mismatch: $($remote.name)"
      }
      $target = Join-Path $tempRoot $remote.name
      Invoke-WebRequest -UseBasicParsing -Uri $remote.browser_download_url -OutFile $target -MaximumRedirection 5 -TimeoutSec 300
      if ((Get-Sha256 $target) -ne (Get-Sha256 $expected.FullName)) {
        throw "Remote release asset hash mismatch: $($remote.name)"
      }
    }
  } finally {
    Remove-Item -LiteralPath $tempRoot -Recurse -Force -ErrorAction SilentlyContinue
  }
}

$branch = (& git -C $repoRoot branch --show-current).Trim()
if ($LASTEXITCODE -ne 0 -or $branch -ne "main") { throw "Desktop releases must run from main" }
$dirty = @(& git -C $repoRoot status --porcelain --untracked-files=normal)
if ($LASTEXITCODE -ne 0 -or $dirty.Count -gt 0) { throw "Desktop releases require a clean source worktree" }
$head = (& git -C $repoRoot rev-parse HEAD).Trim()
$remoteHead = (& git -C $repoRoot rev-parse origin/main).Trim()
if ($LASTEXITCODE -ne 0 -or $head -notmatch '^[0-9a-f]{40}$' -or $head -ne $remoteHead) {
  throw "Local main must exactly match origin/main before publishing"
}

$manifest = [System.IO.File]::ReadAllText($manifestPath, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
if ($manifest.sourceCommit -ne $head -or $manifest.dirty -or $manifest.platform -ne "windows-x64") {
  throw "Artifact manifest is not a clean build of the current main commit"
}
$tag = "v$($manifest.version)"
$releaseFiles = @()
foreach ($declared in @($manifest.deliverables)) {
  $path = Join-Path $artifactsDir $declared.name
  if (-not (Test-Path $path -PathType Leaf)) { throw "Release asset is missing: $path" }
  $file = Get-Item -LiteralPath $path
  $hash = Get-Sha256 $path
  if ($file.Length -ne [long]$declared.bytes -or $hash -ne [string]$declared.sha256) {
    throw "Release asset does not match the manifest: $($declared.name)"
  }
  $releaseFiles += $file
}
$releaseFiles += Get-Item -LiteralPath $manifestPath
$releaseFiles += Get-Item -LiteralPath $checksumPath
$releaseFiles = @($releaseFiles | Sort-Object Name)

$notes = @"
Convenient Window Desktop $($manifest.version) for Windows 11 x64.

- Per-user NSIS installer and portable ZIP are built from public source commit $head.
- SHA-256 values are recorded in SHA256SUMS and artifact-manifest.json.
- The executable and installer are currently unsigned; Windows may show an unknown-publisher or SmartScreen warning.
- This pre-release is intended for download, installation, portable, and uninstall acceptance before the same immutable assets are promoted to a stable release.
"@

if ($DryRun) {
  $operation = if ($Promote) { "promotion" } else { "pre-release" }
  Write-Output "Dry run: $Repository $tag $operation"
  $releaseFiles | Select-Object Name, Length
  return
}

$credentialInput = "protocol=https`nhost=github.com`n`n"
$credential = $credentialInput | git credential fill 2>$null
$tokenLine = $credential | Where-Object { $_ -like "password=*" } | Select-Object -First 1
if (-not $tokenLine) { throw "No GitHub credential is available from git credential manager" }
$token = $tokenLine.Substring("password=".Length)
$headers = @{
  Accept = "application/vnd.github+json"
  Authorization = "Bearer $token"
  "X-GitHub-Api-Version" = "2022-11-28"
  "User-Agent" = "ConvenientWindowDesktopRelease"
}
$api = "https://api.github.com/repos/$Repository"
$release = $null

try {
  if ($Promote) {
    $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag"
    if (-not $release.prerelease -or $release.draft) { throw "$tag is not an active pre-release" }
    if ([string]$release.target_commitish -ne $head) { throw "$tag does not target the current main commit" }
    Assert-RemoteAssets -Release $release -ExpectedFiles $releaseFiles
    $payload = @{ prerelease = $false; draft = $false; make_latest = "true" } | ConvertTo-Json
    $release = Invoke-RestMethod -UseBasicParsing -Method Patch -Headers $headers -ContentType "application/json" -Body $payload -Uri "$api/releases/$($release.id)"
    Write-Output "Promoted without replacing assets: $($release.html_url)"
    return
  }

  try {
    Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag" | Out-Null
    throw "$tag already exists; release assets will not be overwritten"
  } catch {
    if ($_.Exception.Message -like "$tag already exists*") { throw }
    if (-not $_.Exception.Response -or [int]$_.Exception.Response.StatusCode -ne 404) { throw }
  }

  $payload = @{
    tag_name = $tag
    target_commitish = $head
    name = "Convenient Window Desktop $($manifest.version)"
    body = $notes
    draft = $false
    prerelease = $true
    make_latest = "false"
  } | ConvertTo-Json
  $release = Invoke-RestMethod -UseBasicParsing -Method Post -Headers $headers -ContentType "application/json" -Body $payload -Uri "$api/releases"
  $uploadBase = ($release.upload_url -replace '\{\?name,label\}$', '')
  foreach ($asset in $releaseFiles) {
    $encodedName = [Uri]::EscapeDataString($asset.Name)
    Invoke-RestMethod -UseBasicParsing -Method Post -Headers $headers -ContentType "application/octet-stream" -InFile $asset.FullName -Uri "${uploadBase}?name=$encodedName" | Out-Null
    Write-Output "Uploaded: $($asset.Name)"
  }
  $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag"
  Assert-RemoteAssets -Release $release -ExpectedFiles $releaseFiles
  Write-Output "Published pre-release: $($release.html_url)"
} catch {
  if (-not $Promote -and $release -and $release.id) {
    try { Invoke-RestMethod -UseBasicParsing -Method Delete -Headers $headers -Uri "$api/releases/$($release.id)" | Out-Null } catch {}
    try { Invoke-RestMethod -UseBasicParsing -Method Delete -Headers $headers -Uri "$api/git/refs/tags/$tag" | Out-Null } catch {}
  }
  throw
} finally {
  $token = $null
  $credential = $null
  $headers.Authorization = $null
}
