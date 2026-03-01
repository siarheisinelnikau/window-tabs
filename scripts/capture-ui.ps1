[CmdletBinding()]
param(
    [ValidateSet('groups','empty','picker','saved','settings','recovery','error')][string[]]$Screen = @('groups','settings','picker','empty','saved','recovery'),
    [string]$OutputDirectory = (Join-Path $PSScriptRoot '..\docs\screenshots'),
    [ValidatePattern('^[0-9]+x[0-9]+$')][string]$Size = '900x640',
    [string]$Executable,
    [switch]$SkipBuild
)
$ErrorActionPreference = 'Stop'
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$taskOutput = [IO.Path]::GetFullPath($OutputDirectory)
$taskExecutable = if ($Executable) { [IO.Path]::GetFullPath($Executable) } else { Join-Path $taskRoot 'target\release\WindowTabs.exe' }
Push-Location -LiteralPath $taskRoot
try {
    if (-not $SkipBuild -and -not $Executable) {
        & cargo build --offline --locked --release --jobs 2 --target-dir (Join-Path $taskRoot 'target')
        if ($LASTEXITCODE -ne 0) { throw 'Capture build failed.' }
    }
    New-Item -ItemType Directory -Force -Path $taskOutput | Out-Null
    $taskEvidence = @()
    $taskExecutableHash=(Get-FileHash -LiteralPath $taskExecutable -Algorithm SHA256).Hash.ToLowerInvariant()
    foreach ($taskScreen in $Screen) {
        $taskPng = Join-Path $taskOutput ($taskScreen + '.png')
        if (Test-Path -LiteralPath $taskPng) { throw "Capture output already exists: $taskPng. Choose a fresh directory." }
        $taskStderr = Join-Path $taskOutput ($taskScreen + '.stderr.txt')
        $taskArguments = @('--capture', ('"' + $taskPng + '"'), '--screen', $taskScreen, '--size', $Size)
        $taskProcess = Start-Process -FilePath $taskExecutable -ArgumentList $taskArguments -WindowStyle Hidden -PassThru -RedirectStandardError $taskStderr
        $taskHandle = $taskProcess.Handle
        if (-not $taskProcess.WaitForExit(20000)) {
            $taskProcess.Kill()
            throw "Only the owned disposable capture was terminated after its 20-second timeout: $taskScreen"
        }
        $taskProcess.Refresh()
        if ($taskProcess.ExitCode -ne 0) { throw "Capture failed for $taskScreen. See $taskStderr" }
        $taskJson = Get-Content -LiteralPath ([IO.Path]::ChangeExtension($taskPng,'capture.json')) -Raw | ConvertFrom-Json
        if ($taskJson.render_passes -ne 1 -or $taskJson.screen -ne $taskScreen) { throw 'Capture completion metadata did not match.' }
        $taskEvidence += [pscustomobject]@{Screen=$taskScreen; ProcessId=$taskProcess.Id; ExitCode=$taskProcess.ExitCode; CpuMilliseconds=$taskProcess.TotalProcessorTime.TotalMilliseconds; RenderMilliseconds=$taskJson.elapsed_ms; RasterPasses=$taskJson.render_passes; Viewport=$taskJson.viewport; ExecutableSha256=$taskExecutableHash; Sha256=(Get-FileHash -LiteralPath $taskPng -Algorithm SHA256).Hash.ToLowerInvariant()}
        $taskProcess.Dispose()
        if ((Get-Item -LiteralPath $taskStderr).Length -eq 0) { Remove-Item -LiteralPath $taskStderr }
    }
    [IO.File]::WriteAllText((Join-Path $taskOutput 'capture-summary.json'), (ConvertTo-Json -InputObject @($taskEvidence) -Depth 5), [Text.UTF8Encoding]::new($false))
    $taskEvidence | Format-Table Screen,ProcessId,ExitCode,CpuMilliseconds,RenderMilliseconds,RasterPasses -AutoSize
} finally {
    Pop-Location
}
