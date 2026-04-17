# Rebuild the original code-native logo as a multi-resolution Windows icon.
# Geometry and colors match assets/window-tabs.svg; no external artwork or fonts.
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$taskRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
function New-TaskRoundedPath([single]$X, [single]$Y, [single]$Width, [single]$Height, [single]$Radius) {
    $taskPath = [Drawing.Drawing2D.GraphicsPath]::new()
    $taskDiameter = 2 * $Radius
    $taskPath.AddArc($X, $Y, $taskDiameter, $taskDiameter, 180, 90)
    $taskPath.AddArc($X + $Width - $taskDiameter, $Y, $taskDiameter, $taskDiameter, 270, 90)
    $taskPath.AddArc($X + $Width - $taskDiameter, $Y + $Height - $taskDiameter, $taskDiameter, $taskDiameter, 0, 90)
    $taskPath.AddArc($X, $Y + $Height - $taskDiameter, $taskDiameter, $taskDiameter, 90, 90)
    $taskPath.CloseFigure()
    return ,$taskPath
}
$taskSizes = @(16, 24, 32, 48, 64, 128, 256)
$taskImages = @()
foreach ($taskSize in $taskSizes) {
    $taskBitmap = [Drawing.Bitmap]::new($taskSize, $taskSize)
    $taskGraphics = [Drawing.Graphics]::FromImage($taskBitmap)
    $taskGraphics.SmoothingMode = [Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $taskGraphics.ScaleTransform($taskSize / 64.0, $taskSize / 64.0)
    $taskBrush = [Drawing.SolidBrush]::new([Drawing.ColorTranslator]::FromHtml('#6354d3'))
    $taskPen = [Drawing.Pen]::new([Drawing.Color]::White, 2.5)
    $taskPath = New-TaskRoundedPath 2 2 60 60 18
    $taskGraphics.FillPath($taskBrush, $taskPath)
    $taskPath.Dispose()
    foreach ($taskTab in @(@(14,15,23), @(23,25,33))) {
        $taskPath = New-TaskRoundedPath $taskTab[0] $taskTab[1] 27 24 3
        $taskGraphics.FillPath($taskBrush, $taskPath)
        $taskGraphics.DrawPath($taskPen, $taskPath)
        $taskGraphics.DrawLine($taskPen, [single]$taskTab[0], [single]$taskTab[2], [single]($taskTab[0]+27), [single]$taskTab[2])
        $taskPath.Dispose()
    }
    $taskStream = [IO.MemoryStream]::new()
    $taskBitmap.Save($taskStream, [Drawing.Imaging.ImageFormat]::Png)
    $taskImages += ,$taskStream.ToArray()
    $taskStream.Dispose()
    $taskPen.Dispose()
    $taskBrush.Dispose()
    $taskGraphics.Dispose()
    $taskBitmap.Dispose()
}
$taskOutput = [IO.MemoryStream]::new()
$taskWriter = [IO.BinaryWriter]::new($taskOutput)
$taskWriter.Write([uint16]0)
$taskWriter.Write([uint16]1)
$taskWriter.Write([uint16]$taskSizes.Count)
$taskOffset = 6 + 16 * $taskSizes.Count
for ($taskIndex=0; $taskIndex -lt $taskSizes.Count; $taskIndex++) {
    $taskDimension = if ($taskSizes[$taskIndex] -eq 256) { 0 } else { $taskSizes[$taskIndex] }
    $taskWriter.Write([byte]$taskDimension)
    $taskWriter.Write([byte]$taskDimension)
    $taskWriter.Write([uint16]0)
    $taskWriter.Write([uint16]1)
    $taskWriter.Write([uint16]32)
    $taskWriter.Write([uint32]$taskImages[$taskIndex].Length)
    $taskWriter.Write([uint32]$taskOffset)
    $taskOffset += $taskImages[$taskIndex].Length
}
foreach ($taskImage in $taskImages) { $taskWriter.Write([byte[]]$taskImage) }
$taskWriter.Flush()
[IO.File]::WriteAllBytes((Join-Path $taskRoot 'assets\window-tabs.ico'), $taskOutput.ToArray())
$taskWriter.Dispose()
$taskOutput.Dispose()
