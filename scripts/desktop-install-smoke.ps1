param(
  [string]$ArtifactsDir,
  [string]$WorkRoot,
  [switch]$KeepData
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

if ([System.Environment]::OSVersion.Platform -ne [System.PlatformID]::Win32NT) {
  throw "NSIS installation smoke can only run on Windows"
}

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
$appPath = Join-Path $installDir "convenient-window.exe"
$uninstallerPath = Join-Path $installDir "uninstall.exe"
$runtimeSmoke = Join-Path $PSScriptRoot "desktop-runtime-smoke.ps1"
$installed = $false
$uninstalled = $false
$failure = $null

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

  Invoke-Uninstall -Uninstaller $uninstallerPath
  $uninstalled = $true
  if (@(Get-MatchingUninstallKeys -InstallDirectory $installDir).Count -ne 0) {
    throw "NSIS uninstall registry entry remained after uninstall"
  }
  $remainingShortcuts = @(Get-MatchingShortcuts -InstallDirectory $installDir)
  if ($remainingShortcuts.Count -ne 0) {
    throw "NSIS shortcuts remained after uninstall: $($remainingShortcuts -join ', ')"
  }

  Write-Output "NSIS install: exit=0; executable and complete helper payload verified"
  Write-Output "installed runtime: helper ready, schema v7 persisted, graceful exit"
  Write-Output "NSIS uninstall: exit=0; install directory, registry entry, and shortcuts removed"
  Write-Output "installer shortcuts observed: $($installedShortcuts.Count)"
} catch {
  $failure = $_
} finally {
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
