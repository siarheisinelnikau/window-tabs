[CmdletBinding()]
param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $taskRoot
try {
    & cargo fmt --all --check
    if ($LASTEXITCODE -ne 0) { throw 'Formatting check failed.' }
    $taskArgs = @('--locked', '--jobs', '2', '--target-dir', (Join-Path $taskRoot 'target'))
    if ($Offline) { $taskArgs += '--offline' }
    & cargo test @taskArgs --lib --tests
    if ($LASTEXITCODE -ne 0) { throw 'Synthetic tests failed.' }
    & cargo clippy @taskArgs --all-targets -- -D warnings
    if ($LASTEXITCODE -ne 0) { throw 'Clippy failed.' }
} finally {
    Pop-Location
}

