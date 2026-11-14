param([string] $Name = 'lazybuddy')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName PresentationCore,WindowsBase

$assets = Join-Path $PSScriptRoot '../assets'
[xml]$svg = Get-Content -Raw -LiteralPath (Join-Path $assets ($Name+'.svg'))
$visual = [Windows.Media.DrawingVisual]::new()
$context = $visual.RenderOpen()
try {
    foreach ($path in $svg.SelectNodes("/*[local-name()='svg']/*[local-name()='path']")) {
        $geometry = [Windows.Media.Geometry]::Parse($path.GetAttribute('d'))
        $color = [Windows.Media.ColorConverter]::ConvertFromString($path.GetAttribute('fill'))
        $context.DrawGeometry([Windows.Media.SolidColorBrush]::new($color), $null, $geometry)
    }
} finally {
    $context.Close()
}

$sizes = @(16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
$frames = foreach ($size in $sizes) {
    $dpi = $size * 96.0 / 64
    $bitmap = [Windows.Media.Imaging.RenderTargetBitmap]::new(
        $size, $size, $dpi, $dpi, [Windows.Media.PixelFormats]::Pbgra32
    )
    $bitmap.Render($visual)
    $encoder = [Windows.Media.Imaging.PngBitmapEncoder]::new()
    $encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
    $frame = [IO.MemoryStream]::new()
    try {
        $encoder.Save($frame)
        ,$frame.ToArray()
    } finally {
        $frame.Dispose()
    }
}

$stream = [IO.MemoryStream]::new()
$writer = [IO.BinaryWriter]::new($stream)
try {
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$sizes.Count)
    $offset = 6 + 16 * $sizes.Count
    for ($i = 0; $i -lt $sizes.Count; $i++) {
        $size = [byte]($sizes[$i] % 256)
        $length = $frames[$i].Length
        $writer.Write($size)
        $writer.Write($size)
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$length)
        $writer.Write([uint32]$offset)
        $offset += $length
    }
    foreach ($frame in $frames) { $writer.Write([byte[]]$frame) }
    [IO.File]::WriteAllBytes((Join-Path $assets ($Name+'.ico')), $stream.ToArray())
} finally {
    $writer.Dispose()
}
