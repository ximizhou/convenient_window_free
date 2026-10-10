param(
  [string]$Repository = "ximizhou/convenient_window_free",
  [switch]$Promote,
  [switch]$DryRun,
  [switch]$ReplacePreRelease,
  [switch]$RequireTrustedSignature
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($ReplacePreRelease -and $Promote) { throw "-ReplacePreRelease cannot be combined with -Promote" }
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

function Get-ReleaseWithExpectedAssets {
  param(
    [Parameter(Mandatory = $true)][string]$Uri,
    [Parameter(Mandatory = $true)][hashtable]$Headers,
    [Parameter(Mandatory = $true)][System.IO.FileInfo[]]$ExpectedFiles
  )

  $expectedNames = (($ExpectedFiles.Name | Sort-Object) -join "`n")
  for ($attempt = 1; $attempt -le 5; $attempt += 1) {
    $candidate = Invoke-RestMethod -UseBasicParsing -Headers $Headers -Uri $Uri
    $actualNames = ((@($candidate.assets).name | Sort-Object) -join "`n")
    if ($actualNames -ceq $expectedNames) { return $candidate }
    if ($attempt -lt 5) { Start-Sleep -Seconds $attempt }
  }
  throw "Remote release asset set did not converge to the local candidate"
}

# 捕获原生命令输出的版本（用于 git 这类需要读取 stdout 的调用）。
# 在 $ErrorActionPreference = "Stop" 下，PowerShell 5.1 会把原生命令写在 stderr 的任何输出
# 包装成 NativeCommandError 并当作终止错误抛出 —— git 的 `LF will be replaced by CRLF`
# 就是一条普通警告，却会让发布脚本在读取工作树状态时直接中断。
# 必须先按记录类型打标再过滤：直接 `"$_"` 会把 ErrorRecord 变成普通字符串，
# 警告文本就混进返回值，把「工作树是否干净」的判断污染掉。
function Invoke-NativeCapture {
  param([Parameter(Mandatory = $true)][string[]]$CommandLine)
  $previous = $ErrorActionPreference
  $ErrorActionPreference = "Continue"
  try {
    $executable = $CommandLine[0]
    $arguments = @()
    if ($CommandLine.Count -gt 1) { $arguments = $CommandLine[1..($CommandLine.Count - 1)] }
    $tagged = & $executable @arguments 2>&1 | ForEach-Object {
      if ($_ -is [System.Management.Automation.ErrorRecord]) {
        [pscustomobject]@{ IsError = $true; Text = "$_" }
      } else {
        [pscustomobject]@{ IsError = $false; Text = "$_" }
      }
    }
    return @($tagged | Where-Object { -not $_.IsError } | ForEach-Object { $_.Text })
  } finally {
    $ErrorActionPreference = $previous
  }
}

function Get-SourceChanges {
  $unstaged = @(Invoke-NativeCapture @("git", "-C", $repoRoot, "diff", "--name-only", "--"))
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect unstaged source changes" }
  $staged = @(Invoke-NativeCapture @("git", "-C", $repoRoot, "diff", "--cached", "--name-only", "--"))
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect staged source changes" }
  $untracked = @(Invoke-NativeCapture @("git", "-C", $repoRoot, "ls-files", "--others", "--exclude-standard"))
  if ($LASTEXITCODE -ne 0) { throw "Unable to inspect untracked source files" }
  return @($unstaged + $staged + $untracked | Where-Object { $_ } | Sort-Object -Unique)
}

$branchOutput = Invoke-NativeCapture @("git", "-C", $repoRoot, "branch", "--show-current")
$branch = ([string]($branchOutput | Select-Object -First 1)).Trim()
if ($LASTEXITCODE -ne 0 -or $branch -ne "main") { throw "Desktop releases must run from main" }
$dirty = @(Get-SourceChanges)
if ($dirty.Count -gt 0) { throw "Desktop releases require a clean source worktree" }
$headOutput = Invoke-NativeCapture @("git", "-C", $repoRoot, "rev-parse", "HEAD")
$head = ([string]($headOutput | Select-Object -First 1)).Trim()
$remoteHeadOutput = Invoke-NativeCapture @("git", "-C", $repoRoot, "rev-parse", "origin/main")
$remoteHead = ([string]($remoteHeadOutput | Select-Object -First 1)).Trim()
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

if ($RequireTrustedSignature) {
  $signatureCommand = Get-Command Get-AuthenticodeSignature -ErrorAction SilentlyContinue
  if (-not $signatureCommand) { throw "Get-AuthenticodeSignature is required when -RequireTrustedSignature is set" }
  $portableDir = Join-Path $artifactsDir "ConvenientWindow-portable"
  $signedBinaries = @(
    (Join-Path $portableDir "ConvenientWindow.exe"),
    (Join-Path $portableDir "helper\magic-corners-helper.exe"),
    ($releaseFiles | Where-Object { $_.Name -like '*setup.exe' } | Select-Object -First 1).FullName
  )
  foreach ($binary in $signedBinaries) {
    $signature = & $signatureCommand $binary
    if (-not $signature -or $signature.Status -ne [System.Management.Automation.SignatureStatus]::Valid) {
      throw "Trusted Authenticode signature required for $binary; status=$($signature.Status)"
    }
  }
}

$signingNote = if ($RequireTrustedSignature) {
  "- Authenticode signatures were required and verified for the application, helper, and installer."
} else {
  "- The executable, helper, and installer are unsigned; Windows may show an unknown-publisher or SmartScreen warning."
}
# 用户可见的更新说明单独放在 docs/release-notes/<version>.md，由脚本嵌入 Release 正文。
# 这样发版时只需写一次该文件，Pre-release、转正和后续复核都会带上同一份说明；
# 文件缺失时给出显式提示而不是静默漏掉（0.6.0 首次发布就因为说明只写在内部文档里、
# 没有进 Release 正文而被指出）。
$releaseNotesPath = Join-Path $repoRoot "docs\release-notes\$($manifest.version).md"
if (Test-Path -LiteralPath $releaseNotesPath) {
  $releaseNotes = (Get-Content -LiteralPath $releaseNotesPath -Raw -Encoding utf8).Trim()
  $whatsNewSection = "## What's new`n`n$releaseNotes"
} else {
  Write-Warning "Release notes are missing: $releaseNotesPath（Release 正文将缺少用户可见的更新说明）"
  $whatsNewSection = "## What's new`n`n- (no user-visible changes recorded for this version)"
}

# Promote 会复用同一份 $notes，因此状态行必须随操作切换：转正后若仍写着
# "This pre-release is intended for ... before promoted to a stable release"，
# 说明与状态自相矛盾。
$statusLine = if ($Promote) {
  "- Stable release. The assets are unchanged from the accepted pre-release; nothing was rebuilt or re-uploaded."
} elseif ($ReplacePreRelease) {
  "- Replaceable pre-release candidate. This candidate may be refreshed while it remains a pre-release; stable promotion locks the assets."
} else {
  "- This pre-release is intended for download, installation, portable, and uninstall acceptance and may be refreshed before stable promotion."
}

$notes = @"
Convenient Window Desktop $($manifest.version) for Windows 11 x64.

$whatsNewSection

## Assets

- Per-user NSIS installer and portable ZIP are built from public source commit $head.
- SHA-256 values are recorded in SHA256SUMS and artifact-manifest.json.
$signingNote
$statusLine
"@

if ($DryRun) {
  $operation = if ($Promote) { "promotion" } elseif ($ReplacePreRelease) { "pre-release replacement" } else { "pre-release" }
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
$createdRelease = $false

try {
  if ($Promote) {
    $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag"
    if ($release.draft) { throw "$tag is still a draft" }
    if ([string]$release.target_commitish -ne $head) { throw "$tag does not target the current main commit" }
    Assert-RemoteAssets -Release $release -ExpectedFiles $releaseFiles
    # 只改状态与文案，不上传、不替换任何资产。
    # PowerShell 5.1 会把字符串 -Body 按 ISO-8859-1 编码发送，非 ASCII 内容会变成 "?"。
    # 一律转成 UTF-8 字节数组再发送，保证 Release 正文在任何语言下都完整。
    $payload = [System.Text.Encoding]::UTF8.GetBytes((@{
      prerelease = $false
      draft = $false
      make_latest = "true"
      body = $notes
    } | ConvertTo-Json))
    $release = Invoke-RestMethod -UseBasicParsing -Method Patch -Headers $headers -ContentType "application/json; charset=utf-8" -Body $payload -Uri "$api/releases/$($release.id)"
    Write-Output "Promoted without replacing assets: $($release.html_url)"
    return
  }

  if ($ReplacePreRelease) {
    $release = Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag"
    if ($release.draft -or -not $release.prerelease) {
      throw "$tag is not an active pre-release; stable assets cannot be replaced"
    }
    $refPayload = [System.Text.Encoding]::UTF8.GetBytes((@{ sha = $head; force = $true } | ConvertTo-Json))
    Invoke-RestMethod -UseBasicParsing -Method Patch -Headers $headers -ContentType "application/json; charset=utf-8" -Body $refPayload -Uri "$api/git/refs/tags/$tag" | Out-Null
    foreach ($remoteAsset in @($release.assets)) {
      Invoke-RestMethod -UseBasicParsing -Method Delete -Headers $headers -Uri "$api/releases/assets/$($remoteAsset.id)" | Out-Null
    }
    $releasePayload = [System.Text.Encoding]::UTF8.GetBytes((@{
      target_commitish = $head
      name = "Convenient Window Desktop $($manifest.version)"
      body = $notes
      draft = $false
      prerelease = $true
      make_latest = "false"
    } | ConvertTo-Json))
    $release = Invoke-RestMethod -UseBasicParsing -Method Patch -Headers $headers -ContentType "application/json; charset=utf-8" -Body $releasePayload -Uri "$api/releases/$($release.id)"
    Write-Output "Refreshing existing pre-release assets: $tag"
  } else {
    try {
      Invoke-RestMethod -UseBasicParsing -Headers $headers -Uri "$api/releases/tags/$tag" | Out-Null
      throw "$tag already exists; use -ReplacePreRelease while it is still a pre-release"
    } catch {
      if ($_.Exception.Message -like "$tag already exists*") { throw }
      if (-not $_.Exception.Response -or [int]$_.Exception.Response.StatusCode -ne 404) { throw }
    }
    $payload = [System.Text.Encoding]::UTF8.GetBytes((@{
      tag_name = $tag
      target_commitish = $head
      name = "Convenient Window Desktop $($manifest.version)"
      body = $notes
      draft = $false
      prerelease = $true
      make_latest = "false"
    } | ConvertTo-Json))
    $release = Invoke-RestMethod -UseBasicParsing -Method Post -Headers $headers -ContentType "application/json; charset=utf-8" -Body $payload -Uri "$api/releases"
    $createdRelease = $true
  }
  $uploadBase = ($release.upload_url -replace '\{\?name,label\}$', '')
  foreach ($asset in $releaseFiles) {
    $encodedName = [Uri]::EscapeDataString($asset.Name)
    Invoke-RestMethod -UseBasicParsing -Method Post -Headers $headers -ContentType "application/octet-stream" -InFile $asset.FullName -Uri "${uploadBase}?name=$encodedName" | Out-Null
    Write-Output "Uploaded: $($asset.Name)"
  }
  $release = Get-ReleaseWithExpectedAssets -Uri "$api/releases/tags/$tag" -Headers $headers -ExpectedFiles $releaseFiles
  Assert-RemoteAssets -Release $release -ExpectedFiles $releaseFiles
  Write-Output "Published pre-release: $($release.html_url)"
} catch {
  if ($createdRelease -and $release -and $release.id) {
    try { Invoke-RestMethod -UseBasicParsing -Method Delete -Headers $headers -Uri "$api/releases/$($release.id)" | Out-Null } catch {}
    try { Invoke-RestMethod -UseBasicParsing -Method Delete -Headers $headers -Uri "$api/git/refs/tags/$tag" | Out-Null } catch {}
  }
  throw
} finally {
  $token = $null
  $credential = $null
  $headers.Authorization = $null
}
