param([string]$ScriptPath = (Join-Path $PSScriptRoot 'publish-desktop-release.ps1'))
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$tokens = $null
$parseErrors = $null
$ast = [Management.Automation.Language.Parser]::ParseFile($ScriptPath, [ref]$tokens, [ref]$parseErrors)
if ($parseErrors.Count) { throw "Release script syntax error: $($parseErrors[0].Message)" }
$parameters = (Get-Command Invoke-RestMethod -CommandType Cmdlet).Parameters
$calls = @($ast.FindAll({
  param($node)
  $node -is [Management.Automation.Language.CommandAst] -and $node.GetCommandName() -eq 'Invoke-RestMethod'
}, $true))
if ($calls.Count -eq 0) { throw 'No release API calls were checked' }
foreach ($call in $calls) {
  foreach ($element in $call.CommandElements) {
    if ($element -is [Management.Automation.Language.CommandParameterAst] -and
        -not $parameters.ContainsKey($element.ParameterName)) {
      throw "Unknown or abbreviated REST parameter -$($element.ParameterName) at line $($element.Extent.StartLineNumber)"
    }
  }
}
Write-Output "release API parameter regression: $($calls.Count) calls passed; no network or credentials used"
