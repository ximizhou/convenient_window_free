function Read-TextFileWithRetry {
  param(
    [Parameter(Mandatory = $true)][string]$Path,
    [int]$TimeoutMilliseconds = 5000
  )

  $deadline = [DateTime]::UtcNow.AddMilliseconds($TimeoutMilliseconds)
  do {
    try {
      return [System.IO.File]::ReadAllText($Path, [System.Text.Encoding]::UTF8)
    } catch [System.IO.IOException] {
      if ([DateTime]::UtcNow -ge $deadline) { throw }
      Start-Sleep -Milliseconds 100
    }
  } while ($true)
}
