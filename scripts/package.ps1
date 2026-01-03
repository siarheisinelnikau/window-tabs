[CmdletBinding()]
param([switch]$Offline)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $taskRoot
try {
    $taskVersionMatch = Select-String -LiteralPath (Join-Path $taskRoot 'Cargo.toml') -Pattern '^version = "([0-9]+\.[0-9]+\.[0-9]+)"$' | Select-Object -First 1
    if (-not $taskVersionMatch) { throw 'Cannot find the stable package version.' }
    $taskVersion = $taskVersionMatch.Matches[0].Groups[1].Value
    $taskTarget = Join-Path $taskRoot 'target'
    $taskArgs = @('build', '--release', '--locked', '--jobs', '2', '--target', 'x86_64-pc-windows-msvc', '--target-dir', $taskTarget)
    if ($Offline) { $taskArgs += '--offline' }
    & cargo @taskArgs
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $taskBinary = Join-Path $taskTarget 'x86_64-pc-windows-msvc\release\WindowTabs.exe'
    & (Join-Path $PSScriptRoot 'inspect-pe.ps1') -Path $taskBinary
    $taskOutput = Join-Path $taskRoot "dist\v$taskVersion"
    New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
    $taskName = "WindowTabs-v$taskVersion.exe"
    $taskAsset = Join-Path $taskOutput $taskName
    Copy-Item -LiteralPath $taskBinary -Destination $taskAsset -Force
    $taskHash = (Get-FileHash -LiteralPath $taskAsset -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText((Join-Path $taskOutput 'SHA256SUMS.txt'), "$taskHash  $taskName`n", [Text.UTF8Encoding]::new($false))
    Copy-Item -LiteralPath (Join-Path $taskRoot 'LICENSE') -Destination $taskOutput -Force
    Copy-Item -LiteralPath (Join-Path $taskRoot 'THIRD_PARTY_NOTICES.md') -Destination $taskOutput -Force
    Write-Output "Packaged $taskAsset"
    Write-Output "SHA-256: $taskHash"
} finally {
    Pop-Location
}

