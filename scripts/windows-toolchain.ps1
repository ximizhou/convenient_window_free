function Initialize-MsvcEnvironment {
  [CmdletBinding()]
  param()

  if (Get-Command cl.exe -ErrorAction SilentlyContinue) { return }

  $candidates = @()
  if ($env:VSINSTALLDIR) { $candidates += $env:VSINSTALLDIR }

  $vswhereCandidates = @(
    (Join-Path ${env:ProgramFiles(x86)} "Microsoft Visual Studio\Installer\vswhere.exe"),
    (Get-Command vswhere.exe -ErrorAction SilentlyContinue | Select-Object -ExpandProperty Source -ErrorAction SilentlyContinue)
  ) | Where-Object { $_ -and (Test-Path $_) }
  foreach ($vswhere in $vswhereCandidates) {
    $installation = (& $vswhere -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath).Trim()
    if ($installation) { $candidates += $installation }
  }

  $vsDevCmd = $candidates |
    Select-Object -Unique |
    ForEach-Object { Join-Path $_ "Common7\Tools\VsDevCmd.bat" } |
    Where-Object { Test-Path $_ } |
    Select-Object -First 1
  if (-not $vsDevCmd) {
    throw "MSVC Build Tools with the x64 C++ toolset were not found"
  }

  $command = "`"$vsDevCmd`" -arch=x64 -host_arch=x64 -no_logo && set"
  $environment = & $env:ComSpec /d /s /c $command
  if ($LASTEXITCODE -ne 0) { throw "VsDevCmd failed with exit code $LASTEXITCODE" }
  foreach ($line in $environment) {
    $separator = $line.IndexOf("=")
    if ($separator -le 0) { continue }
    $name = $line.Substring(0, $separator)
    $value = $line.Substring($separator + 1)
    [System.Environment]::SetEnvironmentVariable($name, $value, "Process")
  }

  $cl = Get-Command cl.exe -ErrorAction SilentlyContinue
  $link = Get-Command link.exe -ErrorAction SilentlyContinue
  if (-not $cl -or -not $link) {
    throw "VsDevCmd completed but cl.exe/link.exe are still unavailable"
  }
}
