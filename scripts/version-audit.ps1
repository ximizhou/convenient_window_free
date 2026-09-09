param(
  [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot),
  [string]$PluginPackagePath,
  [string]$PluginManifestPath,
  [string]$HelperAssetsPath
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

function Read-JsonVersion([string]$Path, [string]$Label) {
  if (-not $Path) { return $null }
  $full = [System.IO.Path]::GetFullPath($Path)
  if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "$Label not found: $full" }
  $text = [System.IO.File]::ReadAllText($full, [System.Text.Encoding]::UTF8)
  $match = [regex]::Match($text, '"version"\s*:\s*"([^"]+)"')
  if (-not $match.Success) { throw "$Label has no version: $full" }
  return [string]$match.Groups[1].Value
}
function Read-CargoVersion([string]$Path, [string]$Label) {
  $full = [System.IO.Path]::GetFullPath($Path)
  if (-not (Test-Path -LiteralPath $full -PathType Leaf)) { throw "$Label not found: $full" }
  $match = Select-String -LiteralPath $full -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1
  if (-not $match) { throw "$Label has no package version: $full" }
  return [string]$match.Matches[0].Groups[1].Value
}

$root = [System.IO.Path]::GetFullPath($RepositoryRoot)
$values = [ordered]@{}
$values["workspace"] = Read-JsonVersion (Join-Path $root 'package.json') 'workspace package'
$values["desktop package"] = Read-JsonVersion (Join-Path $root 'apps/desktop/package.json') 'desktop package'
$values["desktop cargo"] = Read-CargoVersion (Join-Path $root 'apps/desktop/src-tauri/Cargo.toml') 'desktop Cargo.toml'
$values["tauri"] = Read-JsonVersion (Join-Path $root 'apps/desktop/src-tauri/tauri.conf.json') 'Tauri config'
$values["helper cargo"] = Read-CargoVersion (Join-Path $root 'helper/Cargo.toml') 'helper Cargo.toml'
if ($PluginPackagePath) { $values["plugin package"] = Read-JsonVersion $PluginPackagePath 'plugin package' }
if ($PluginManifestPath) { $values["plugin manifest"] = Read-JsonVersion $PluginManifestPath 'plugin manifest' }
if ($HelperAssetsPath) { $values["helper assets"] = Read-JsonVersion $HelperAssetsPath 'helper assets' }

$unique = @($values.Values | Sort-Object -Unique)
if ($unique.Count -ne 1) {
  $values.GetEnumerator() | ForEach-Object { Write-Output ("{0}: {1}" -f $_.Key, $_.Value) }
  throw "version audit failed: inconsistent versions"
}
$values.GetEnumerator() | ForEach-Object { Write-Output ("{0}: {1}" -f $_.Key, $_.Value) }
Write-Output ("version audit: passed ({0})" -f $unique[0])

