[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
Push-Location -LiteralPath $taskRoot
try {
    $taskJson = & cargo metadata --offline --locked --format-version 1
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read locked package metadata.' }
    $taskMetadata = $taskJson | ConvertFrom-Json
    $taskText = [Text.StringBuilder]::new()
    [void]$taskText.AppendLine('# Third-party notices')
    [void]$taskText.AppendLine()
    [void]$taskText.AppendLine('Generated from Cargo.lock by scripts/update-notices.ps1. MIT is selected where offered; Zlib-only packages retain their Zlib license. Build-only and target-specific packages are included for completeness. Unicode data licensing is reproduced in addition to MIT where required.')
    foreach ($taskPackage in ($taskMetadata.packages | Sort-Object name)) {
        if ($taskPackage.name -eq 'window-tabs') { continue }
        if ($taskPackage.license -notmatch 'MIT' -and $taskPackage.license -ne 'Zlib') { throw "Review the license for $($taskPackage.name)." }
        $taskPackageRoot = Split-Path -Parent $taskPackage.manifest_path
        $taskFiles = @(Get-ChildItem -LiteralPath $taskPackageRoot -File | Where-Object { $_.Name -match '^(LICENSE-MIT(\.md)?|license-mit|LICENSE-UNICODE)$' })
        if ($taskFiles.Count -eq 0 -and $taskPackage.license -in @('MIT','Zlib')) { $taskFiles = @(Get-ChildItem -LiteralPath $taskPackageRoot -File | Where-Object { $_.Name -match '^LICENSE(\.md|\.txt)?$' }) }
        if ($taskFiles.Count -eq 0) { throw "No MIT license file for $($taskPackage.name)." }
        [void]$taskText.AppendLine()
        [void]$taskText.AppendLine("## $($taskPackage.name) $($taskPackage.version)")
        foreach ($taskFile in $taskFiles) {
            [void]$taskText.AppendLine()
            [void]$taskText.AppendLine('```text')
            [void]$taskText.AppendLine(([regex]::Replace([IO.File]::ReadAllText($taskFile.FullName), '(?m)[ \t]+(?=\r?$)', '')).Trim())
            [void]$taskText.AppendLine('```')
        }
    }
    [IO.File]::WriteAllText((Join-Path $taskRoot 'THIRD_PARTY_NOTICES.md'), $taskText.ToString(), [Text.UTF8Encoding]::new($false))
} finally {
    Pop-Location
}
