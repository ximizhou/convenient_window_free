param([Parameter(Mandatory = $true)][string]$HelperPath)
$ErrorActionPreference = 'Stop'
$HelperPath = (Resolve-Path -LiteralPath $HelperPath).Path
$payloadDir = Split-Path -Parent $HelperPath
if (-not (Test-Path -LiteralPath (Join-Path $payloadDir 'libunwind.dll')) -or @(Get-ChildItem -LiteralPath $payloadDir -Filter 'std-*.dll' -File).Count -eq 0) {
  throw 'Use a packaged helper directory containing libunwind.dll and std-*.dll'
}
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('convenient-window-owner-' + [guid]::NewGuid().ToString('N'))
$owner = $null
$helper = $null
$stopEvent = $null
New-Item -ItemType Directory -Path $testRoot | Out-Null
try {
  foreach ($scenario in @('owner-exit', 'stop-event', 'stale-owner')) {
    $dataDir = Join-Path $testRoot $scenario
    New-Item -ItemType Directory -Path $dataDir | Out-Null
    $owner = Start-Process powershell.exe -ArgumentList @('-NoProfile', '-NonInteractive', '-Command', 'Start-Sleep -Seconds 60') -WindowStyle Hidden -PassThru
    $birth = $owner.StartTime.ToFileTimeUtc()
    if ($scenario -eq 'stale-owner') { $birth += 1 }
    $eventName = 'Local\ConvenientWindow.OwnerTest.' + [guid]::NewGuid().ToString('N')
    $stopEvent = [Threading.EventWaitHandle]::new($false, [Threading.EventResetMode]::ManualReset, $eventName)
    $helper = Start-Process -FilePath $HelperPath -ArgumentList @('--data-dir', ('"' + $dataDir + '"'), '--desktop-owner', $owner.Id, $birth, '--desktop-stop-event', $eventName) -WindowStyle Hidden -PassThru
    if ($scenario -eq 'stale-owner') {
      if (-not $helper.WaitForExit(5000) -or $helper.ExitCode -eq 0) { throw 'Stale desktop identity was accepted' }
    } else {
      $deadline = [DateTime]::UtcNow.AddSeconds(8)
      $log = Join-Path $dataDir 'magic-corners-helper.log'
      do {
        if ($helper.HasExited) { throw "Helper exited before readiness in $scenario" }
        if ((Test-Path -LiteralPath $log) -and [IO.File]::ReadAllText($log).Contains('websocket: listening')) { break }
        Start-Sleep -Milliseconds 100
      } while ([DateTime]::UtcNow -lt $deadline)
      if (-not (Test-Path -LiteralPath $log) -or -not [IO.File]::ReadAllText($log).Contains('websocket: listening')) { throw 'Helper never became ready' }
      if ($scenario -eq 'owner-exit') { Stop-Process -Id $owner.Id -Force } else { [void]$stopEvent.Set() }
      if (-not $helper.WaitForExit(6000) -or $helper.ExitCode -ne 0) { throw "Helper did not exit cleanly after $scenario" }
      if (-not [IO.File]::ReadAllText($log).Contains('main: desktop owner exited or requested stop')) { throw 'Owner monitor did not request shutdown' }
    }
    Write-Output "$scenario`: passed"
    if (-not $owner.HasExited) { Stop-Process -Id $owner.Id -Force }
    [void]$owner.WaitForExit(5000)
    $stopEvent.Dispose()
    $stopEvent = $null
  }
} finally {
  if ($stopEvent) { [void]$stopEvent.Set(); $stopEvent.Dispose() }
  if ($owner -and -not $owner.HasExited) { Stop-Process -Id $owner.Id -Force; [void]$owner.WaitForExit(5000) }
  if ($helper -and -not $helper.HasExited) { [void]$helper.WaitForExit(6000) }
  if ($helper -and -not $helper.HasExited) { Stop-Process -Id $helper.Id -Force; [void]$helper.WaitForExit(5000) }
  $resolved = [IO.Path]::GetFullPath($testRoot)
  $tempParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
  if (-not $resolved.StartsWith($tempParent, [StringComparison]::OrdinalIgnoreCase)) { throw 'Unexpected cleanup location' }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
