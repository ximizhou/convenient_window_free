$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "read-text-file-with-retry.ps1")

$workRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("convenient-window-log-read-" + [guid]::NewGuid().ToString("N"))
$logPath = Join-Path $workRoot "helper.log"
$readyPath = Join-Path $workRoot "locked"
$job = $null

try {
  New-Item -ItemType Directory -Path $workRoot | Out-Null
  [System.IO.File]::WriteAllText($logPath, "helper-ready", [System.Text.Encoding]::UTF8)
  $job = Start-Job -ScriptBlock {
    param($Path, $ReadyPath)
    $stream = [System.IO.File]::Open($Path, [System.IO.FileMode]::Open, [System.IO.FileAccess]::ReadWrite, [System.IO.FileShare]::None)
    try {
      [System.IO.File]::WriteAllText($ReadyPath, "locked")
      Start-Sleep -Milliseconds 700
    } finally {
      $stream.Dispose()
    }
  } -ArgumentList $logPath, $readyPath

  $deadline = [DateTime]::UtcNow.AddSeconds(5)
  while (-not (Test-Path -LiteralPath $readyPath)) {
    if ([DateTime]::UtcNow -ge $deadline) { throw "Lock holder did not become ready" }
    Start-Sleep -Milliseconds 25
  }

  $stopwatch = [System.Diagnostics.Stopwatch]::StartNew()
  $content = Read-TextFileWithRetry -Path $logPath -TimeoutMilliseconds 3000
  $stopwatch.Stop()
  if ($content -ne "helper-ready") { throw "Retry reader returned unexpected content" }
  if ($stopwatch.ElapsedMilliseconds -lt 400) { throw "Retry reader did not wait for the sharing violation to clear" }
  Write-Output "log read retry: passed after $($stopwatch.ElapsedMilliseconds)ms"
} finally {
  if ($job) {
    Wait-Job -Job $job -Timeout 5 | Out-Null
    Receive-Job -Job $job -ErrorAction SilentlyContinue | Out-Null
    Remove-Job -Job $job -Force -ErrorAction SilentlyContinue
  }
  if (Test-Path -LiteralPath $workRoot) {
    Remove-Item -LiteralPath $workRoot -Recurse -Force
  }
}
