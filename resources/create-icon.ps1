# Rebuild the multi-resolution Windows icon using only Windows drawing APIs.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$frames = [System.Collections.Generic.List[object]]::new()
foreach ($size in @(16, 20, 24, 32, 40, 48, 64, 128, 256)) {
    $bitmap = [System.Drawing.Bitmap]::new($size, $size)
    $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
    $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
    $graphics.Clear([System.Drawing.Color]::Transparent)
    $graphics.ScaleTransform($size / 64.0, $size / 64.0)
    $path = [System.Drawing.Drawing2D.GraphicsPath]::new()
    $path.AddArc(2, 2, 24, 24, 180, 90)
    $path.AddArc(38, 2, 24, 24, 270, 90)
    $path.AddArc(38, 38, 24, 24, 0, 90)
    $path.AddArc(2, 38, 24, 24, 90, 90)
    $path.CloseFigure()
    $ink = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#242B29'))
    $cream = [System.Drawing.Pen]::new([System.Drawing.ColorTranslator]::FromHtml('#F4F1E9'), 4)
    $cream.StartCap = [System.Drawing.Drawing2D.LineCap]::Square
    $cream.EndCap = [System.Drawing.Drawing2D.LineCap]::Square
    $accent = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#EA784A'))
    $graphics.FillPath($ink, $path)
    $left = [System.Drawing.PointF[]]@([System.Drawing.PointF]::new(24,21), [System.Drawing.PointF]::new(17,21), [System.Drawing.PointF]::new(17,43), [System.Drawing.PointF]::new(24,43))
    $right = [System.Drawing.PointF[]]@([System.Drawing.PointF]::new(40,21), [System.Drawing.PointF]::new(47,21), [System.Drawing.PointF]::new(47,43), [System.Drawing.PointF]::new(40,43))
    $graphics.DrawLines($cream, $left)
    $graphics.DrawLines($cream, $right)
    $graphics.FillEllipse($accent, 28, 28, 8, 8)
    $buffer = [System.IO.MemoryStream]::new()
    $bitmap.Save($buffer, [System.Drawing.Imaging.ImageFormat]::Png)
    $frames.Add(@{ Size = $size; Bytes = $buffer.ToArray() })
    if ($size -eq 256) { $bitmap.Save((Join-Path $env:TEMP 'ai-skillview-icon.png'), [System.Drawing.Imaging.ImageFormat]::Png) }
    $buffer.Dispose()
    $accent.Dispose()
    $cream.Dispose()
    $ink.Dispose()
    $path.Dispose()
    $graphics.Dispose()
    $bitmap.Dispose()
}
$stream = [System.IO.File]::Create((Join-Path $PSScriptRoot 'app.ico'))
$writer = [System.IO.BinaryWriter]::new($stream)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$frames.Count)
    $offset = 6 + 16 * $frames.Count
    foreach ($frame in $frames) {
        $dimension = if ($frame.Size -eq 256) { 0 } else { $frame.Size }
        $writer.Write([byte]$dimension)
        $writer.Write([byte]$dimension)
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$frame.Bytes.Length)
        $writer.Write([uint32]$offset)
        $offset += $frame.Bytes.Length
    }
    foreach ($frame in $frames) { $writer.Write([byte[]]$frame.Bytes) }
}
finally {
    $writer.Dispose()
    $stream.Dispose()
}
