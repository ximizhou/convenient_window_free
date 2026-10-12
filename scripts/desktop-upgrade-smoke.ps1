param(
  [Parameter(Mandatory = $true)][string]$LegacyInstaller,
  [Parameter(Mandatory = $true)][string]$Installer
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$legacyName = [string]([char]0x4fbf) + [char]0x6377 + [char]0x7a97 + [char]0x53e3
$newName = 'Convenient Window'
$uninstallRoot = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall'
foreach ($name in @($legacyName, $newName)) {
  if (Test-Path -LiteralPath "$uninstallRoot\$name") { throw "An existing $name installation must be tested in a separate Windows account" }
}
if (Get-Process -Name convenient-window, ConvenientWindow, magic-corners-helper -ErrorAction SilentlyContinue) {
  throw 'Close the application and helper before running upgrade acceptance'
}
$LegacyInstaller = (Resolve-Path -LiteralPath $LegacyInstaller).Path
$Installer = (Resolve-Path -LiteralPath $Installer).Path
$tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\')
$work = Join-Path $tempRoot ('convenient-window-upgrade-' + [guid]::NewGuid().ToString('N'))
$install = Join-Path $work ('Custom path ' + $legacyName)
$sentinel = Join-Path $install 'user-file.txt'
$productNames = @($legacyName, $newName)
$backups = @{}
$failure = $null
$installed = $false
$previousDataRoot = $env:CONVENIENT_WINDOW_DATA_DIR
$previousExitDelay = $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS

function Run-Installer([string]$Path, [string[]]$Arguments) {
  $process = Start-Process -FilePath $Path -ArgumentList $Arguments -PassThru -Wait -WindowStyle Hidden
  if ($process.ExitCode -ne 0) { throw "Installer exited with $($process.ExitCode): $Path" }
}

New-Item -ItemType Directory -Path $work | Out-Null
try {
  # Restore any stale product-location records left by earlier uninstallations.
  foreach ($name in $productNames) {
    $key = "HKCU\Software\ximizhou\$name"
    if (Test-Path -LiteralPath "Registry::$key") {
      $backup = Join-Path $work ($backups.Count.ToString() + '.reg')
      & reg.exe export $key $backup /y | Out-Null
      if ($LASTEXITCODE -ne 0) { throw 'Could not back up product registration' }
      $backups[$key] = $backup
    } else { $backups[$key] = $null }
  }
  # A prior uninstall can leave a product-location key pointing at a removed directory.
  $staleKey = "HKCU:\Software\ximizhou\$newName"
  New-Item -Path $staleKey -Force | Out-Null
  Set-Item -LiteralPath $staleKey -Value (Join-Path $work 'removed-installation')
  Run-Installer $LegacyInstaller @('/S', "/D=$install")
  $installed = $true
  if (-not (Test-Path -LiteralPath "$uninstallRoot\$legacyName")) { throw 'Legacy registration was not created' }
  [IO.File]::WriteAllText($sentinel, 'preserve user files')
  Run-Installer $Installer @('/S')
  if (Test-Path -LiteralPath "$uninstallRoot\$legacyName") { throw 'Legacy uninstall registration survived migration' }
  $registration = Get-ItemProperty -LiteralPath "$uninstallRoot\$newName"
  if ($registration.InstallLocation.Trim('"') -ne $install) { throw 'Upgrade changed the existing custom installation path' }
  if ([IO.File]::ReadAllText($sentinel) -ne 'preserve user files') { throw 'Upgrade changed a user file' }
  if (-not (Test-Path -LiteralPath (Join-Path $install 'convenient-window.exe'))) { throw 'Upgraded executable is missing' }

  # Seed an old configuration and verify the packaged app upgrades it on startup.
  $data = Join-Path $work 'data'
  New-Item -ItemType Directory -Path $data | Out-Null
  $fixture = Get-Content -LiteralPath (Join-Path $PSScriptRoot '../tests/fixtures/i18n-migration.json') -Raw -Encoding UTF8 | ConvertFrom-Json
  $settings = Join-Path $data 'desktop-settings.json'
  [IO.File]::WriteAllText($settings, ($fixture.input | ConvertTo-Json -Depth 30), [Text.UTF8Encoding]::new($false))
  $env:CONVENIENT_WINDOW_DATA_DIR = $data
  $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS = '15000'
  $app = Start-Process -FilePath (Join-Path $install 'convenient-window.exe') -PassThru -WindowStyle Hidden
  if (-not $app.WaitForExit(30000)) { Stop-Process -Id $app.Id; throw 'Upgraded app did not exit' }
  if ($app.ExitCode -ne 0) { throw "Upgraded app exited with $($app.ExitCode)" }
  $desktopConfig = Get-Content -LiteralPath $settings -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($desktopConfig.schemaVersion -ne 9) { throw 'Desktop settings did not migrate to schema 9' }
  $backupConfig = Get-Content -LiteralPath ($settings + '.bak') -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($backupConfig.schemaVersion -ne 7) { throw 'Original schema 7 backup was not retained' }
  $helperConfig = Join-Path $data 'helper-data/config.json'
  if (-not (Test-Path -LiteralPath $helperConfig)) { throw 'Upgraded helper did not persist its configuration' }
  $migrated = Get-Content -LiteralPath $helperConfig -Raw -Encoding UTF8 | ConvertFrom-Json
  if ($migrated.schemaVersion -ne 9) { throw 'Upgrade did not migrate schema 7 to schema 9' }
  foreach ($gesture in $migrated.mouseGestures.gestures) {
    $expected = $fixture.expectedNames.PSObject.Properties[$gesture.id].Value
    $nameProperty = $gesture.PSObject.Properties['name']
    $actual = if ($nameProperty) { $nameProperty.Value } else { $null }
    if ($actual -ne $expected) { throw "Gesture name migration failed: $($gesture.id)" }
    if ($gesture.action.value -ne 'Ctrl+K') { throw 'Gesture action changed during migration' }
  }
  Run-Installer $Installer @('/S')
  Run-Installer (Join-Path $install 'uninstall.exe') @('/S')
  $installed = $false
  if (Test-Path -LiteralPath "$uninstallRoot\$newName") { throw 'Uninstall registration remains' }
  if (-not (Test-Path -LiteralPath $sentinel)) { throw 'Uninstaller removed an unrelated user file' }
  if (-not (Test-Path -LiteralPath $settings)) { throw 'Uninstaller removed application data' }
  Write-Output 'upgrade acceptance: legacy custom path, schema migration, user files, reinstall and uninstall passed'
} catch {
  $failure = $_
} finally {
  $env:CONVENIENT_WINDOW_DATA_DIR = $previousDataRoot
  $env:CONVENIENT_WINDOW_SMOKE_EXIT_MS = $previousExitDelay
  if ($installed -and (Test-Path -LiteralPath (Join-Path $install 'uninstall.exe'))) {
    Run-Installer (Join-Path $install 'uninstall.exe') @('/S')
  }
  foreach ($key in $backups.Keys) {
    if (Test-Path -LiteralPath "Registry::$key") { Remove-Item -LiteralPath "Registry::$key" -Recurse -Force }
    if ($backups[$key]) {
      & reg.exe import $backups[$key] | Out-Null
      if ($LASTEXITCODE -ne 0) { throw 'Could not restore product registration' }
    }
  }
  $resolved = [IO.Path]::GetFullPath($work)
  if (-not $resolved.StartsWith($tempRoot + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid cleanup root' }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
if ($failure) { throw $failure }
