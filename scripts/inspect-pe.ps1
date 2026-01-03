[CmdletBinding()]
param([Parameter(Mandatory)][string]$Path)
$ErrorActionPreference = 'Stop'
$taskBytes = [IO.File]::ReadAllBytes([IO.Path]::GetFullPath($Path))
if ($taskBytes.Length -lt 256 -or $taskBytes[0] -ne 0x4d -or $taskBytes[1] -ne 0x5a) { throw 'Missing DOS header.' }
$taskPe = [BitConverter]::ToInt32($taskBytes, 0x3c)
if ($taskPe -lt 0 -or $taskPe + 96 -ge $taskBytes.Length) { throw 'Invalid PE header offset.' }
if ([BitConverter]::ToUInt32($taskBytes, $taskPe) -ne 0x00004550) { throw 'Missing PE signature.' }
if ([BitConverter]::ToUInt16($taskBytes, $taskPe + 4) -ne 0x8664) { throw 'Expected x64 machine type.' }
if ([BitConverter]::ToUInt16($taskBytes, $taskPe + 24) -ne 0x020b) { throw 'Expected PE32+ image.' }
if ([BitConverter]::ToUInt16($taskBytes, $taskPe + 24 + 68) -ne 2) { throw 'Expected Windows GUI subsystem.' }
$taskText = [Text.Encoding]::UTF8.GetString($taskBytes)
if (-not $taskText.Contains('PerMonitorV2') -or -not $taskText.Contains('asInvoker')) { throw 'Expected embedded DPI and privilege manifest.' }
$taskSectionCount = [BitConverter]::ToUInt16($taskBytes, $taskPe + 6)
$taskSections = $taskPe + 24 + [BitConverter]::ToUInt16($taskBytes, $taskPe + 20)
function Convert-TaskRva([uint32]$Rva) {
    for ($taskIndex = 0; $taskIndex -lt $taskSectionCount; $taskIndex++) {
        $taskSection = $taskSections + 40 * $taskIndex
        $taskVirtualSize = [BitConverter]::ToUInt32($taskBytes, $taskSection + 8)
        $taskVirtualAddress = [BitConverter]::ToUInt32($taskBytes, $taskSection + 12)
        $taskRawSize = [BitConverter]::ToUInt32($taskBytes, $taskSection + 16)
        $taskRaw = [BitConverter]::ToUInt32($taskBytes, $taskSection + 20)
        if ($Rva -ge $taskVirtualAddress -and $Rva -lt $taskVirtualAddress + [Math]::Max($taskVirtualSize, $taskRawSize)) {
            return [int]($taskRaw + $Rva - $taskVirtualAddress)
        }
    }
    throw 'PE virtual address is outside the section table.'
}
$taskImportRva = [BitConverter]::ToUInt32($taskBytes, $taskPe + 24 + 112 + 8)
$taskImportOffset = Convert-TaskRva $taskImportRva
$taskImports = @()
for ($taskIndex = 0; $taskIndex -lt 128; $taskIndex++) {
    $taskDescriptor = $taskImportOffset + 20 * $taskIndex
    $taskNameRva = [BitConverter]::ToUInt32($taskBytes, $taskDescriptor + 12)
    if ($taskNameRva -eq 0) { break }
    $taskNameOffset = Convert-TaskRva $taskNameRva
    $taskEnd = $taskNameOffset
    while ($taskEnd -lt $taskBytes.Length -and $taskBytes[$taskEnd] -ne 0 -and $taskEnd - $taskNameOffset -le 260) { $taskEnd++ }
    if ($taskEnd -ge $taskBytes.Length -or $taskEnd - $taskNameOffset -gt 260) { throw 'Invalid import name.' }
    $taskImports += [Text.Encoding]::ASCII.GetString($taskBytes, $taskNameOffset, $taskEnd - $taskNameOffset)
}
if ($taskImports | Where-Object { $_ -match '^(vcruntime|msvcp|ucrtbase|api-ms-win-crt-)' }) { throw 'An external MSVC runtime DLL was imported.' }
[pscustomobject]@{Path = [IO.Path]::GetFullPath($Path); Architecture = 'x64'; Subsystem = 'Windows GUI'; Bytes = $taskBytes.Length; Manifest = 'PerMonitorV2 / asInvoker'; Imports = ($taskImports -join ', '); CRT = 'No external MSVC runtime DLL imports'}

