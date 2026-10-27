# Rebuild installer artwork from the existing application icon.
[CmdletBinding()]
param([string]$PreviewPath)

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$icon = [System.Drawing.Icon]::new((Join-Path $PSScriptRoot 'app.ico'), 256, 256)
$source = $icon.ToBitmap()
$wizard = [System.Drawing.Bitmap]::new(164, 314, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$header = [System.Drawing.Bitmap]::new(55, 55, [System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
$titleFont = [System.Drawing.Font]::new('Segoe UI Semibold', 16, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$bodyFont = [System.Drawing.Font]::new('Segoe UI', 12, [System.Drawing.FontStyle]::Regular, [System.Drawing.GraphicsUnit]::Pixel)
$ink = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#242B29'))
$muted = [System.Drawing.SolidBrush]::new([System.Drawing.ColorTranslator]::FromHtml('#6C716D'))
$format = [System.Drawing.StringFormat]::new()
$format.Alignment = [System.Drawing.StringAlignment]::Center
$format.LineAlignment = [System.Drawing.StringAlignment]::Center

try {
    $graphics = [System.Drawing.Graphics]::FromImage($wizard)
    try {
        $graphics.Clear([System.Drawing.ColorTranslator]::FromHtml('#F4F3EF'))
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
        $graphics.DrawImage($source, [System.Drawing.Rectangle]::new(42, 88, 80, 80))
        $graphics.DrawString('AI Skillview', $titleFont, $ink, [System.Drawing.RectangleF]::new(12, 186, 140, 28), $format)
        $graphics.DrawString("Good prompts,`nalways within reach.", $bodyFont, $muted, [System.Drawing.RectangleF]::new(12, 220, 140, 44), $format)
    }
    finally {
        $graphics.Dispose()
    }

    $graphics = [System.Drawing.Graphics]::FromImage($header)
    try {
        $graphics.Clear([System.Drawing.Color]::White)
        $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
        $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
        $graphics.DrawImage($source, [System.Drawing.Rectangle]::new(7, 7, 40, 40))
    }
    finally {
        $graphics.Dispose()
    }

    $wizard.Save((Join-Path $PSScriptRoot 'setup-wizard.bmp'), [System.Drawing.Imaging.ImageFormat]::Bmp)
    $header.Save((Join-Path $PSScriptRoot 'setup-header.bmp'), [System.Drawing.Imaging.ImageFormat]::Bmp)
    if ($PreviewPath) {
        $wizard.Save($PreviewPath, [System.Drawing.Imaging.ImageFormat]::Png)
    }
}
finally {
    $format.Dispose()
    $muted.Dispose()
    $ink.Dispose()
    $bodyFont.Dispose()
    $titleFont.Dispose()
    $header.Dispose()
    $wizard.Dispose()
    $source.Dispose()
    $icon.Dispose()
}
