param(
  [string]$RepositoryRoot = (Split-Path -Parent $PSScriptRoot)
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = [System.IO.Path]::GetFullPath($RepositoryRoot)
if (-not (Test-Path -LiteralPath (Join-Path $repoRoot ".git") -PathType Leaf) -and
    -not (Test-Path -LiteralPath (Join-Path $repoRoot ".git") -PathType Container)) {
  throw "RepositoryRoot is not a Git worktree: $repoRoot"
}

Push-Location $repoRoot
try {
  $sourcePaths = @(& git ls-files --cached --others --exclude-standard)
  if ($LASTEXITCODE -ne 0) { throw "Unable to enumerate public source files" }

  $forbiddenPathPattern = '(?i)(^|/)(?:AGENTS|CLAUDE|BLOCKED|PROGRESS|DEVELOPMENT-ROADMAP|WORKFLOW)\.md$|(^|/)(?:auth-token|config\.json|.*\.log|.*\.env|.*\.pem|.*\.p12|.*\.pfx)$'
  $forbiddenPaths = @($sourcePaths | Where-Object {
    $_ -match $forbiddenPathPattern -and (Test-Path -LiteralPath (Join-Path $repoRoot $_) -PathType Leaf)
  })
  if ($forbiddenPaths.Count -gt 0) {
    throw "Forbidden private or generated files are present in public source: $($forbiddenPaths -join ', ')"
  }

  $slash = [regex]::Escape([string][char]92)
  $patterns = @(
    'github_pat_[A-Za-z0-9_]+' ,
    'ghp_[A-Za-z0-9]+' ,
    'BEGIN (RSA |EC |OPENSSH )?PRIVATE KEY',
    "(?i)[A-Z]$slash(?:Users|项目|biancheng)$slash"
  )
  $matches = @()
  foreach ($relativePath in $sourcePaths) {
    $fullPath = Join-Path $repoRoot $relativePath
    if (-not (Test-Path -LiteralPath $fullPath -PathType Leaf)) { continue }
    $bytes = [System.IO.File]::ReadAllBytes($fullPath)
    if ($bytes -contains 0) { continue }
    $content = [System.Text.Encoding]::UTF8.GetString($bytes)
    foreach ($pattern in $patterns) {
      if ($content -match $pattern) {
        $matches += $relativePath
        break
      }
    }
  }
  if ($matches.Count -gt 0) {
    throw "Potential private material found in public source: $((($matches | Sort-Object -Unique) -join ', '))"
  }

  Write-Output "public source audit: passed"
  Write-Output "files scanned: $($sourcePaths.Count)"
} finally {
  Pop-Location
}
