param(
  [Parameter(Mandatory = $true)]
  [string]$AppPath,
  [string]$DataRoot,
  [switch]$ExpectConflict,
  [switch]$ForceAppKill,
  [switch]$KeepData
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

. (Join-Path $PSScriptRoot "read-text-file-with-retry.ps1")

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
      throw "Conflict holder helper did not send its ready message"
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
    # Runtime/status broadcasts may already be queued ahead of the close frame.
    # The shared cancellation token bounds the entire drain, including shutdown.
    do {
      $closed = $socket.ReceiveAsync($receiveBuffer, $timeout.Token).GetAwaiter().GetResult()
    } while ($closed.MessageType -ne [System.Net.WebSockets.WebSocketMessageType]::Close)
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

function Remove-IsolatedDirectory {
  param([Parameter(Mandatory = $true)][string]$Path)
  if (-not (Test-Path -LiteralPath $Path)) { return }
  $resolved = (Resolve-Path -LiteralPath $Path).Path
  $temporaryRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd("\") + "\"
  if ($resolved -ne $Path -or -not $resolved.StartsWith($temporaryRoot, [StringComparison]::OrdinalIgnoreCase)) {
    throw "Unexpected smoke cleanup directory: $Path"
  }
  if ((Get-Item -LiteralPath $resolved).Attributes -band [IO.FileAttributes]::ReparsePoint) {
    throw "Smoke cleanup cannot follow a reparse point: $Path"
  }
  # WebView2 subprocesses can release temporary profile files shortly after the
  # host exits. Retry this isolated directory only; never stop browser processes.
  $deadline = [DateTime]::UtcNow.AddSeconds(10)
  while (Test-Path -LiteralPath $resolved) {
    try {
      Remove-Item -LiteralPath $resolved -Recurse -Force -ErrorAction Stop
    } catch {
      if ([DateTime]::UtcNow -ge $deadline) { throw }
      Start-Sleep -Milliseconds 200
    }
  }
}

if ($ExpectConflict -and $ForceAppKill) {
  throw "ExpectConflict and ForceAppKill cannot be combined"
}

$AppPath = (Resolve-Path $AppPath).Path
if (-not $DataRoot) {
  $DataRoot = Join-Path ([System.IO.Path]::GetTempPath()) ("convenient-window-runtime-" + [guid]::NewGuid().ToString("N"))
}
$DataRoot = [System.IO.Path]::GetFullPath($DataRoot)
$allowedRoots = @([System.IO.Path]::GetFullPath([System.IO.Path]::GetTempPath()).TrimEnd("\"))
$insideAllowedRoot = $allowedRoots | Where-Object {
  $DataRoot.StartsWith($_ + "\", [System.StringComparison]::OrdinalIgnoreCase)
} | Select-Object -First 1
if (-not $insideAllowedRoot) { throw "DataRoot must be inside a designated temporary directory: $DataRoot" }
if (Test-Path $DataRoot) { throw "DataRoot must not already exist: $DataRoot" }
New-Item -ItemType Directory -Force -Path $DataRoot | Out-Null

$helperDataRoot = Join-Path $DataRoot "helper-data"
$webviewDataRoot = Join-Path $DataRoot "webview-data"
$holderRoot = "$DataRoot-holder"
$holderDataRoot = Join-Path $holderRoot "helper-data"
$holderProcess = $null
$holderToken = $null
$process = $null
$holderCleanupError = $null
$realAppDataRoot = Join-Path ([System.Environment]::GetFolderPath("LocalApplicationData")) "com.ximizhou.convenientwindow"
$smokeStartedAt = [DateTime]::UtcNow.AddSeconds(-1)
$previousDataRoot = $env:CONVENIENT_WINDOW_DATA_DIR
$previousExitDelay = $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS
try {
  if ($ExpectConflict -and @(Get-Process magic-corners-helper -ErrorAction SilentlyContinue).Count -eq 0) {
    $holderHelper = Join-Path (Split-Path -Parent $AppPath) "helper\magic-corners-helper.exe"
    if (-not (Test-Path $holderHelper -PathType Leaf)) { throw "Conflict holder helper is missing: $holderHelper" }
    if (Test-Path $holderRoot) { throw "Conflict holder root already exists: $holderRoot" }
    New-Item -ItemType Directory -Force -Path $holderDataRoot | Out-Null
    $holderProcess = Start-Process -FilePath $holderHelper -ArgumentList @("--data-dir", "`"$holderDataRoot`"") -PassThru -WindowStyle Hidden
    $holderTokenPath = Join-Path $holderDataRoot "auth-token"
    $holderDeadline = [DateTime]::UtcNow.AddSeconds(5)
    while ([DateTime]::UtcNow -lt $holderDeadline) {
      if ($holderProcess.HasExited) { throw "Conflict holder helper exited before acquiring the lock" }
      if (Test-Path $holderTokenPath) {
        $holderToken = [System.IO.File]::ReadAllText($holderTokenPath).Trim()
        if ($holderToken.Length -eq 64) { break }
      }
      Start-Sleep -Milliseconds 50
    }
    if (-not $holderToken -or $holderToken.Length -ne 64) { throw "Conflict holder helper did not create a valid token" }
  }

  $env:CONVENIENT_WINDOW_DATA_DIR = $DataRoot
  if ($ForceAppKill) {
    Remove-Item Env:CONVENIENT_WINDOW_SMOKE_EXIT_MS -ErrorAction SilentlyContinue
  } else {
    $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS = "7000"
  }
  $process = Start-Process -FilePath $AppPath -ArgumentList "--autostart" -PassThru -WindowStyle Hidden
  $logPath = Join-Path $helperDataRoot "magic-corners-helper.log"
  if ($ForceAppKill) {
    $readyDeadline = [DateTime]::UtcNow.AddSeconds(12)
    $ready = $false
    while ([DateTime]::UtcNow -lt $readyDeadline) {
      if ($process.HasExited) { throw "Desktop app exited before the force-kill lifecycle check" }
      if (Test-Path $logPath) {
        $content = Read-TextFileWithRetry -Path $logPath -TimeoutMilliseconds 500
        if ($content.Contains("websocket: listening 127.0.0.1:56873")) {
          $ready = $true
          break
        }
      }
      Start-Sleep -Milliseconds 100
    }
    if (-not $ready) { throw "Desktop helper did not become ready before the force-kill lifecycle check" }

    Stop-Process -Id $process.Id -Force
    if (-not $process.WaitForExit(5000)) { throw "Desktop app survived the forced termination" }

    $payloadHelperPath = Join-Path (Split-Path -Parent $AppPath) "helper\magic-corners-helper.exe"
    $helperDeadline = [DateTime]::UtcNow.AddSeconds(5)
    do {
      $remainingAfterKill = @(Get-Process magic-corners-helper -ErrorAction SilentlyContinue | Where-Object {
        try { $_.Path -eq $payloadHelperPath } catch { $false }
      })
      if ($remainingAfterKill.Count -eq 0) { break }
      Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $helperDeadline)
    if ($remainingAfterKill.Count -ne 0) {
      throw "Desktop helper survived after its owning app was force-killed"
    }

    $client = [System.Net.Sockets.TcpClient]::new()
    try {
      try {
        $client.Connect("127.0.0.1", 56873)
        if ($client.Connected) {
          throw "Desktop helper port remained open after the owning app was force-killed"
        }
      } catch [System.Net.Sockets.SocketException] {
        # Connection refused is the expected post-kill state.
      }
    } finally {
      $client.Dispose()
    }
    Write-Output "desktop force-kill: owner exited; Job Object removed sidecar; port closed"
  } else {
    if (-not $process.WaitForExit(20000)) {
      Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
      throw "Desktop app did not exit through its managed smoke path"
    }
    if ($process.ExitCode -ne 0) { throw "Desktop app exited with code $($process.ExitCode)" }
  }

  $log = Get-Item $logPath -ErrorAction SilentlyContinue
  if (-not $log) { throw "Desktop helper log was not created under the explicit isolated data directory" }
  if (-not (Test-Path $webviewDataRoot -PathType Container)) {
    throw "Desktop WebView data was not isolated under the explicit data directory"
  }
  $logContent = Read-TextFileWithRetry -Path $log.FullName
  if ($ForceAppKill) {
    if (-not $logContent.Contains("websocket: listening 127.0.0.1:56873")) {
      throw "Desktop helper never reached its listening state before the forced termination"
    }
  } elseif ($ExpectConflict) {
    if (-not $logContent.Contains("HELPER_INSTANCE_CONFLICT")) {
      throw "Expected helper lock conflict marker was not recorded"
    }
    Write-Output "desktop runtime conflict: detected HELPER_INSTANCE_CONFLICT"
  } else {
    if (-not $logContent.Contains("websocket: listening 127.0.0.1:56873")) {
      throw "Desktop helper never reached its listening state"
    }
    if (-not $logContent.Contains("main: websocket server stopped")) {
      throw "Desktop helper did not stop through its normal shutdown path"
    }
    $token = Get-Item (Join-Path $helperDataRoot "auth-token") -ErrorAction SilentlyContinue
    $config = Get-Item (Join-Path $helperDataRoot "config.json") -ErrorAction SilentlyContinue
    $tokenValue = ""
    if ($token) {
      $tokenValue = [System.IO.File]::ReadAllText($token.FullName)
      $tokenValue = $tokenValue.Trim()
    }
    if (-not $token -or $tokenValue.Length -ne 64) {
      throw "Desktop helper token was not created correctly"
    }
    if (-not $config) { throw "Desktop schema v9 configuration was not persisted" }
    $configValue = [System.IO.File]::ReadAllText($config.FullName, [System.Text.Encoding]::UTF8) | ConvertFrom-Json
    if ($configValue.schemaVersion -ne 9) { throw "Desktop configuration did not preserve schema v9" }
    Write-Output "desktop runtime success: helper listened, schema v9 persisted, graceful stop recorded"
  }

  $payloadHelperPath = Join-Path (Split-Path -Parent $AppPath) "helper\magic-corners-helper.exe"
  $remaining = @(Get-Process magic-corners-helper -ErrorAction SilentlyContinue | Where-Object {
    try {
      $_.Path -eq $payloadHelperPath -and (-not $holderProcess -or $_.Id -ne $holderProcess.Id)
    } catch { $false }
  })
  if ($remaining.Count -gt 0) { throw "Desktop helper process remained after app exit" }
  $realWrites = @()
  if (Test-Path $realAppDataRoot) {
    $realWrites = @(Get-ChildItem $realAppDataRoot -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
      $_.LastWriteTimeUtc -ge $smokeStartedAt
    })
  }
  if ($realWrites.Count -gt 0) {
    throw "Desktop runtime smoke modified the real user profile: $($realWrites.FullName -join ', ')"
  }
  if ($ForceAppKill) {
    Write-Output "desktop app force exit: sidecar remaining=0"
  } else {
    Write-Output "desktop app exit: code=0; sidecar remaining=0"
  }
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
  if ($process -and -not $process.HasExited) {
    Stop-Process -Id $process.Id -Force -ErrorAction SilentlyContinue
    $process.WaitForExit(5000) | Out-Null
  }
  if ($holderProcess -and -not $holderProcess.HasExited) {
    try {
      Stop-HelperGracefully -Token $holderToken
      if (-not $holderProcess.WaitForExit(5000)) {
        throw "Conflict holder helper did not stop cleanly"
      }
      if ($holderProcess.ExitCode -ne 0) {
        throw "Conflict holder helper exited with code $($holderProcess.ExitCode)"
      }
    } catch {
      $holderCleanupError = "Conflict holder helper did not stop gracefully: $_"
      Stop-Process -Id $holderProcess.Id -Force -ErrorAction SilentlyContinue
      $holderProcess.WaitForExit(5000) | Out-Null
    }
  }
  if ((-not $holderProcess) -or $holderProcess.HasExited) {
    if (Test-Path -LiteralPath $holderRoot) {
      Remove-IsolatedDirectory -Path $holderRoot
    }
  }
  if (-not $KeepData) {
    if (Test-Path -LiteralPath $DataRoot) {
      Remove-IsolatedDirectory -Path $DataRoot
    }
    Write-Output "isolated data cleanup: passed"
  }
  if ($holderCleanupError) { throw $holderCleanupError }
}
