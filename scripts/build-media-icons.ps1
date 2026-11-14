$ErrorActionPreference='Stop'
Add-Type -AssemblyName PresentationCore,WindowsBase
$names=@('arrow_left_20_filled','play_20_filled','pause_20_filled','previous_20_filled','next_20_filled','speaker_2_20_filled','speaker_mute_20_filled','star_20_filled','star_20_regular')
foreach($name in $names){
 [xml]$svg=Get-Content -Raw -LiteralPath "assets/button-icons/ic_fluent_$name.svg"
 $drawing=[Windows.Media.DrawingVisual]::new();$context=$drawing.RenderOpen()
 try {foreach($path in $svg.SelectNodes("/*[local-name()='svg']/*[local-name()='path']")){
  $rule=if($path.GetAttribute('fill-rule') -eq 'evenodd'){'F0'}else{'F1'}
  $context.DrawGeometry([Windows.Media.Brushes]::White,$null,[Windows.Media.Geometry]::Parse("$rule $($path.GetAttribute('d'))"))
 }}finally{$context.Close()}
 $bitmap=[Windows.Media.Imaging.RenderTargetBitmap]::new(60,60,288,288,[Windows.Media.PixelFormats]::Pbgra32);$bitmap.Render($drawing)
 $encoder=[Windows.Media.Imaging.PngBitmapEncoder]::new();$encoder.Frames.Add([Windows.Media.Imaging.BitmapFrame]::Create($bitmap))
 $stream=[IO.File]::Create((Join-Path $PWD "assets/button-icons/ic_fluent_$name.png"));try{$encoder.Save($stream)}finally{$stream.Dispose()}
}
