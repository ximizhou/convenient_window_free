param(
  [string]$ArtifactsDir,
  [string]$WorkRoot,
  [switch]$KeepData
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "read-text-file-with-retry.ps1")

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
  throw "NSIS installation smoke can only run on Windows"
}

& (Join-Path $PSScriptRoot "read-text-file-with-retry.test.ps1")

$repoRoot = Split-Path -Parent $PSScriptRoot
if (-not $ArtifactsDir) { $ArtifactsDir = Join-Path $repoRoot "artifacts" }
$installer = Get-ChildItem $ArtifactsDir -Filter "*setup.exe" -File | Select-Object -First 1
if (-not $installer) { throw "NSIS installer is missing from $ArtifactsDir" }

if (-not $WorkRoot) {
  $temporaryBase = if (Test-Path "D:\biancheng\temp") {
    "D:\biancheng\temp"
  } else {
    [System.IO.Path]::GetTempPath()
  }
  $WorkRoot = Join-Path $temporaryBase ("convenient-window-install-" + [guid]::NewGuid().ToString("N"))
}
$WorkRoot = [System.IO.Path]::GetFullPath($WorkRoot)
$allowedRoots = @(
  [System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\"),
  [System.IO.Path]::GetFullPath("D:\biancheng\temp").TrimEnd("\")
)
$insideAllowedRoot = $allowedRoots | Where-Object {
  $WorkRoot.StartsWith($_ + "\", [System.StringComparison]::OrdinalIgnoreCase)
} | Select-Object -First 1
if (-not $insideAllowedRoot) { throw "WorkRoot must be inside a designated temporary directory: $WorkRoot" }
if (Test-Path $WorkRoot) { throw "WorkRoot must not already exist: $WorkRoot" }

$installDir = Join-Path $WorkRoot "install"
$dataRoot = Join-Path $WorkRoot "data"
$liveUninstallDataRoot = Join-Path $WorkRoot "live-uninstall-data"
$externalHelperPayloadRoot = Join-Path $WorkRoot "utools-owned-helper"
$externalHelperDataRoot = Join-Path $WorkRoot "utools-owned-data"
$conflictDesktopDataRoot = Join-Path $WorkRoot "desktop-conflict-data"
$appPath = Join-Path $installDir "convenient-window.exe"
$uninstallerPath = Join-Path $installDir "uninstall.exe"
$runtimeSmoke = Join-Path $PSScriptRoot "desktop-runtime-smoke.ps1"
$installed = $false
$uninstalled = $false
$failure = $null
$liveAppProcess = $null
$liveHelperProcess = $null
$externalHelperProcess = $null
$externalHelperToken = $null
$conflictDesktopProcess = $null
$previousDataRoot = $env:CONVENIENT_WINDOW_DATA_DIR
$previousExitDelay = $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS

function Get-MatchingUninstallKeys {
  param([string]$InstallDirectory)
  $root = "Registry::HKEY_CURRENT_USER\Software\Microsoft\Windows\CurrentVersion\Uninstall"
  if (-not (Test-Path $root)) { return @() }
  return @(Get-ChildItem $root -ErrorAction SilentlyContinue | ForEach-Object {
    $properties = Get-ItemProperty $_.PSPath -ErrorAction SilentlyContinue
    $locationProperty = $properties.PSObject.Properties["InstallLocation"]
    $commandProperty = $properties.PSObject.Properties["UninstallString"]
    $location = if ($locationProperty) { [string]$locationProperty.Value } else { "" }
    $command = if ($commandProperty) { [string]$commandProperty.Value } else { "" }
    if ($location.IndexOf($InstallDirectory, [System.StringComparison]::OrdinalIgnoreCase) -ge 0 -or
        $command.IndexOf($InstallDirectory, [System.StringComparison]::OrdinalIgnoreCase) -ge 0) {
      $_.PSChildName
    }
  })
}

function Get-MatchingShortcuts {
  param([string]$InstallDirectory)
  $shell = New-Object -ComObject WScript.Shell
  $roots = @(
    [System.Environment]::GetFolderPath("StartMenu"),
    [System.Environment]::GetFolderPath("DesktopDirectory")
  ) | Where-Object { $_ -and (Test-Path $_) }
  return @($roots | ForEach-Object {
    Get-ChildItem $_ -Filter "*.lnk" -Recurse -File -ErrorAction SilentlyContinue
  } | Where-Object {
    try {
      $target = $shell.CreateShortcut($_.FullName).TargetPath
      $target.IndexOf($InstallDirectory, [System.StringComparison]::OrdinalIgnoreCase) -ge 0
    } catch {
      $false
    }
  } | Select-Object -ExpandProperty FullName)
}

function Wait-ForRemoval {
  param([string]$Path, [int]$TimeoutMilliseconds)
  $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
  while ([DateTime]::UtcNow -lt $deadline) {
    if (-not (Test-Path $Path)) { return }
    Start-Sleep -Milliseconds 100
  }
  throw "Timed out waiting for removal: $Path"
}

function Stop-HelperGracefully {
  param([Parameter(Mandatory = $true)][string]$Token)
  $socket = [System.Net.WebSockets.ClientWebSocket]::new()
  $timeout = [System.Threading.CancellationTokenSource]::new([TimeSpan]::FromSeconds(5))
  try {
    $socket.Options.AddSubProtocol($Token)
    [void]$socket.ConnectAsync([Uri]"ws://127.0.0.1:56873", $timeout.Token).GetAwaiter().GetResult()
    $buffer = [byte[]]::new(4096)
    $receiveBuffer = [ArraySegment[byte]]::new($buffer)
    $ready = $socket.ReceiveAsync($receiveBuffer, $timeout.Token).GetAwaiter().GetResult()
    if ($ready.MessageType -ne [System.Net.WebSockets.WebSocketMessageType]::Text) {
      throw "External helper did not send its ready message"
    }
    $message = [System.Text.Encoding]::UTF8.GetBytes((@{
      id = [guid]::NewGuid().ToString()
      type = "helper.stop"
      time = [DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()
      data = @{}
    } | ConvertTo-Json -Compress))
    $segment = [ArraySegment[byte]]::new($message)
    [void]$socket.SendAsync(
      $segment,
      [System.Net.WebSockets.WebSocketMessageType]::Text,
      $true,
      $timeout.Token
    ).GetAwaiter().GetResult()
    $closed = $socket.ReceiveAsync($receiveBuffer, $timeout.Token).GetAwaiter().GetResult()
    if ($closed.MessageType -ne [System.Net.WebSockets.WebSocketMessageType]::Close) {
      throw "External helper did not acknowledge its stop request"
    }
    [void]$socket.CloseOutputAsync(
      [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
      "",
      $timeout.Token
    ).GetAwaiter().GetResult()
  } finally {
    $socket.Dispose()
    $timeout.Dispose()
  }
}

function Invoke-Uninstall {
  param([string]$Uninstaller)
  if (-not (Test-Path $Uninstaller -PathType Leaf)) {
    throw "NSIS uninstaller is missing: $Uninstaller"
  }
  $process = Start-Process -FilePath $Uninstaller -ArgumentList "/S" -PassThru -Wait
  if ($process.ExitCode -ne 0) { throw "NSIS uninstaller exited with code $($process.ExitCode)" }
  Wait-ForRemoval -Path $installDir -TimeoutMilliseconds 20000
}

New-Item -ItemType Directory -Force -Path $WorkRoot | Out-Null
try {
  if (@(Get-MatchingUninstallKeys -InstallDirectory $installDir).Count -ne 0) {
    throw "A matching NSIS uninstall registry entry already exists"
  }

  $installProcess = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installDir") -PassThru -Wait
  if ($installProcess.ExitCode -ne 0) { throw "NSIS installer exited with code $($installProcess.ExitCode)" }
  $installed = $true

  if (-not (Test-Path $appPath -PathType Leaf)) { throw "Installed desktop executable is missing: $appPath" }
  if (-not (Test-Path $uninstallerPath -PathType Leaf)) { throw "Installed NSIS uninstaller is missing" }
  if (-not (Test-Path (Join-Path $installDir "helper\magic-corners-helper.exe") -PathType Leaf)) {
    throw "Installed helper executable is missing"
  }
  if (-not (Test-Path (Join-Path $installDir "helper\libunwind.dll") -PathType Leaf)) {
    throw "Installed helper libunwind.dll is missing"
  }
  if (@(Get-ChildItem (Join-Path $installDir "helper") -Filter "std-*.dll" -File).Count -eq 0) {
    throw "Installed helper std-*.dll is missing"
  }

  $registryKeys = @(Get-MatchingUninstallKeys -InstallDirectory $installDir)
  if ($registryKeys.Count -ne 1) {
    throw "Expected one matching NSIS uninstall registry entry, found $($registryKeys.Count)"
  }
  $installedShortcuts = @(Get-MatchingShortcuts -InstallDirectory $installDir)

  & $runtimeSmoke -AppPath $appPath -DataRoot $dataRoot

  $env:CONVENIENT_WINDOW_DATA_DIR = $liveUninstallDataRoot
  Remove-Item Env:CONVENIENT_WINDOW_SMOKE_EXIT_MS -ErrorAction SilentlyContinue
  $liveAppProcess = Start-Process -FilePath $appPath -PassThru
  $liveLogPath = Join-Path $liveUninstallDataRoot "helper-data\magic-corners-helper.log"
  $installedHelperPath = Join-Path $installDir "helper\magic-corners-helper.exe"
  $readyDeadline = [DateTime]::UtcNow.AddSeconds(12)
  $liveReady = $false
  while ([DateTime]::UtcNow -lt $readyDeadline) {
    if ($liveAppProcess.HasExited) { throw "Installed desktop app exited before the live uninstall check" }
    if (Test-Path $liveLogPath) {
      $liveLog = Read-TextFileWithRetry -Path $liveLogPath -TimeoutMilliseconds 500
      if ($liveLog.Contains("websocket: listening 127.0.0.1:56873")) {
        $matchingHelpers = @(Get-Process magic-corners-helper -ErrorAction SilentlyContinue | Where-Object {
          try { $_.Path -eq $installedHelperPath } catch { $false }
        })
        if ($matchingHelpers.Count -eq 1) {
          $liveHelperProcess = $matchingHelpers[0]
          $liveReady = $true
          break
        }
      }
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $liveReady) { throw "Installed desktop helper did not become ready before uninstall" }

  Invoke-Uninstall -Uninstaller $uninstallerPath
  $uninstalled = $true
  if (-not $liveAppProcess.WaitForExit(5000)) {
    throw "Installed desktop app remained after uninstall"
  }
  if (-not $liveHelperProcess.WaitForExit(5000)) {
    throw "Installed desktop helper remained after uninstall"
  }
  $liveLog = Read-TextFileWithRetry -Path $liveLogPath
  if (-not $liveLog.Contains("main: websocket server stopped")) {
    throw "Live uninstall did not stop helper through the graceful shutdown path"
  }
  $client = [System.Net.Sockets.TcpClient]::new()
  try {
    try {
      $client.Connect("127.0.0.1", 56873)
      if ($client.Connected) { throw "Helper port remained open after uninstall" }
    } catch [System.Net.Sockets.SocketException] {
      # Connection refused is the expected post-uninstall state.
    }
  } finally {
    $client.Dispose()
  }
  if (@(Get-MatchingUninstallKeys -InstallDirectory $installDir).Count -ne 0) {
    throw "NSIS uninstall registry entry remained after uninstall"
  }
  $remainingShortcuts = @(Get-MatchingShortcuts -InstallDirectory $installDir)
  if ($remainingShortcuts.Count -ne 0) {
    throw "NSIS shortcuts remained after uninstall: $($remainingShortcuts -join ', ')"
  }

  $reinstallProcess = Start-Process -FilePath $installer.FullName -ArgumentList @("/S", "/D=$installDir") -PassThru -Wait
  if ($reinstallProcess.ExitCode -ne 0) { throw "NSIS reinstall exited with code $($reinstallProcess.ExitCode)" }
  $installed = $true
  $uninstalled = $false
  Copy-Item -Recurse -Force (Join-Path $installDir "helper") $externalHelperPayloadRoot
  New-Item -ItemType Directory -Force -Path $externalHelperDataRoot | Out-Null
  $externalHelperPath = Join-Path $externalHelperPayloadRoot "magic-corners-helper.exe"
  $externalHelperProcess = Start-Process -FilePath $externalHelperPath -ArgumentList @("--data-dir", "`"$externalHelperDataRoot`"") -PassThru
  $externalHelperTokenPath = Join-Path $externalHelperDataRoot "auth-token"
  $externalHelperLogPath = Join-Path $externalHelperDataRoot "magic-corners-helper.log"
  $externalReadyDeadline = [DateTime]::UtcNow.AddSeconds(8)
  $externalReady = $false
  while ([DateTime]::UtcNow -lt $externalReadyDeadline) {
    if ($externalHelperProcess.HasExited) { throw "External uTools-owned helper exited before the non-interference check" }
    if ((Test-Path $externalHelperTokenPath) -and (Test-Path $externalHelperLogPath)) {
      $externalHelperToken = [System.IO.File]::ReadAllText($externalHelperTokenPath).Trim()
      $externalHelperLog = Read-TextFileWithRetry -Path $externalHelperLogPath -TimeoutMilliseconds 500
      if ($externalHelperToken.Length -eq 64 -and $externalHelperLog.Contains("websocket: listening 127.0.0.1:56873")) {
        $externalReady = $true
        break
      }
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $externalReady) { throw "External uTools-owned helper did not become ready" }

  $env:CONVENIENT_WINDOW_DATA_DIR = $conflictDesktopDataRoot
  Remove-Item Env:CONVENIENT_WINDOW_SMOKE_EXIT_MS -ErrorAction SilentlyContinue
  $conflictDesktopProcess = Start-Process -FilePath $appPath -PassThru
  $conflictLogPath = Join-Path $conflictDesktopDataRoot "helper-data\magic-corners-helper.log"
  $conflictDeadline = [DateTime]::UtcNow.AddSeconds(12)
  $conflictRecorded = $false
  while ([DateTime]::UtcNow -lt $conflictDeadline) {
    if ($conflictDesktopProcess.HasExited) { throw "Desktop app exited before the non-interference uninstall check" }
    if (Test-Path $conflictLogPath) {
      $conflictLog = Read-TextFileWithRetry -Path $conflictLogPath -TimeoutMilliseconds 500
      if ($conflictLog.Contains("HELPER_INSTANCE_CONFLICT")) {
        $conflictRecorded = $true
        break
      }
    }
    Start-Sleep -Milliseconds 100
  }
  if (-not $conflictRecorded) { throw "Desktop app did not record the expected external-helper conflict" }

  Invoke-Uninstall -Uninstaller $uninstallerPath
  $uninstalled = $true
  if (-not $conflictDesktopProcess.WaitForExit(5000)) {
    throw "Conflicted desktop app remained after uninstall"
  }
  if ($externalHelperProcess.HasExited) {
    throw "Desktop uninstall terminated the separately owned uTools helper"
  }
  $holderPort = [System.Net.Sockets.TcpClient]::new()
  try {
    $holderPort.Connect("127.0.0.1", 56873)
    if (-not $holderPort.Connected) { throw "External uTools-owned helper port was not available after desktop uninstall" }
  } finally {
    $holderPort.Dispose()
  }
  if (@(Get-MatchingUninstallKeys -InstallDirectory $installDir).Count -ne 0) {
    throw "NSIS uninstall registry entry remained after the non-interference uninstall"
  }
  $remainingShortcuts = @(Get-MatchingShortcuts -InstallDirectory $installDir)
  if ($remainingShortcuts.Count -ne 0) {
    throw "NSIS shortcuts remained after the non-interference uninstall: $($remainingShortcuts -join ', ')"
  }
  Stop-HelperGracefully -Token $externalHelperToken
  if (-not $externalHelperProcess.WaitForExit(5000)) {
    throw "External uTools-owned helper did not stop gracefully after the non-interference check"
  }
  if ($externalHelperProcess.ExitCode -ne 0) {
    throw "External uTools-owned helper exited with code $($externalHelperProcess.ExitCode)"
  }

  Write-Output "NSIS install: exit=0; executable and complete helper payload verified"
  Write-Output "installed runtime: helper ready, schema v7 persisted, graceful exit"
  Write-Output "live uninstall: app and desktop-owned helper exited gracefully; port closed"
  Write-Output "uninstall non-interference: separately owned uTools helper survived and stopped by authenticated IPC"
  Write-Output "NSIS uninstall: exit=0; install directory, registry entry, and shortcuts removed"
  Write-Output "installer shortcuts observed: $($installedShortcuts.Count)"
} catch {
  $failure = $_
} finally {
  if ($null -eq $previousDataRoot) {
    Remove-Item Env:CONVENIENT_WINDOW_DATA_DIR -ErrorAction SilentlyContinue
  } else {
    $env:CONVENIENT_WINDOW_DATA_DIR = $previousDataRoot
  }
  if ($null -eq $previousExitDelay) {
    Remove-Item Env:CONVENIENT_WINDOW_SMOKE_EXIT_MS -ErrorAction SilentlyContinue
  } else {
    $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS = $previousExitDelay
  }
  if ($liveAppProcess -and -not $liveAppProcess.HasExited) {
    Stop-Process -Id $liveAppProcess.Id -Force -ErrorAction SilentlyContinue
    $liveAppProcess.WaitForExit(5000) | Out-Null
  }
  if ($liveHelperProcess -and -not $liveHelperProcess.HasExited) {
    Stop-Process -Id $liveHelperProcess.Id -Force -ErrorAction SilentlyContinue
    $liveHelperProcess.WaitForExit(5000) | Out-Null
  }
  if ($conflictDesktopProcess -and -not $conflictDesktopProcess.HasExited) {
    Stop-Process -Id $conflictDesktopProcess.Id -Force -ErrorAction SilentlyContinue
    $conflictDesktopProcess.WaitForExit(5000) | Out-Null
  }
  if ($externalHelperProcess -and -not $externalHelperProcess.HasExited) {
    if ($externalHelperToken) {
      try {
        Stop-HelperGracefully -Token $externalHelperToken
        $externalHelperProcess.WaitForExit(5000) | Out-Null
      } catch {
        if (-not $failure) { $failure = $_ }
      }
    }
    if (-not $externalHelperProcess.HasExited) {
      Stop-Process -Id $externalHelperProcess.Id -Force -ErrorAction SilentlyContinue
      $externalHelperProcess.WaitForExit(5000) | Out-Null
    }
  }
  if ($installed -and -not $uninstalled -and (Test-Path $uninstallerPath -PathType Leaf)) {
    try {
      Invoke-Uninstall -Uninstaller $uninstallerPath
      $uninstalled = $true
    } catch {
      if (-not $failure) { $failure = $_ }
    }
  }
  if (-not $KeepData -and ((-not $installed) -or $uninstalled)) {
    Remove-Item -Recurse -Force $WorkRoot -ErrorAction SilentlyContinue
    Write-Output "isolated install data cleanup: passed"
  } elseif ($KeepData -or -not $uninstalled) {
    Write-Output "isolated install data retained: $WorkRoot"
  }
}

if ($failure) { throw $failure }
