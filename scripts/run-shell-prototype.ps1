param([switch]$BuildOnly)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$prototype=Join-Path $projectRoot 'apps/shell-prototype'
$output=Join-Path $projectRoot 'target/qa-web-shell'
$sdk=Join-Path $projectRoot 'target/native-player-sdk/PFiles64/gstreamer/1.0/msvc_x86_64'
New-Item -ItemType Directory -Path $output -Force | Out-Null
Push-Location $prototype
try {
    if (!(Test-Path node_modules/.bin/vite.cmd)) {
        npm ci --legacy-peer-deps --no-audit --no-fund
        if($LASTEXITCODE){ throw 'Prototype dependencies failed' }
    }
    npm run build
    if($LASTEXITCODE){ throw 'Prototype UI build failed' }
    $previousTarget=$env:CARGO_TARGET_DIR
    try {
        $env:CARGO_TARGET_DIR=Join-Path $output 'build'
        cargo build --release --locked --manifest-path src-tauri/Cargo.toml
        if($LASTEXITCODE){ throw 'Prototype native build failed' }
    } finally { $env:CARGO_TARGET_DIR=$previousTarget }
} finally { Pop-Location }
$clip=Join-Path $output 'sample.mp4'
if (!(Test-Path -LiteralPath $clip)) {
    $gst=Join-Path $sdk 'bin/gst-launch-1.0.exe'
    if (!(Test-Path -LiteralPath $gst)) { throw 'Synthetic video needs the existing private GStreamer SDK; no SDK is bundled into this prototype.' }
    $savedPath=$env:PATH
    $savedPlugins=$env:GST_PLUGIN_SYSTEM_PATH_1_0
    $savedScanner=$env:GST_PLUGIN_SCANNER_1_0
    $savedRegistry=$env:GST_REGISTRY
    try {
        $env:PATH=(Join-Path $sdk 'bin')+';'+$savedPath
        $env:GST_PLUGIN_SYSTEM_PATH_1_0=Join-Path $sdk 'lib/gstreamer-1.0'
        $env:GST_PLUGIN_SCANNER_1_0=Join-Path $sdk 'libexec/gstreamer-1.0/gst-plugin-scanner.exe'
        $env:GST_REGISTRY=Join-Path $output 'gst-registry.bin'
        & $gst -q videotestsrc pattern=ball foreground-color=4289254837 background-color=4279579937 num-buffers=192 '!' 'video/x-raw,format=I420,width=1280,height=720,framerate=24/1' '!' x264enc speed-preset=ultrafast tune=zerolatency threads=1 key-int-max=24 bitrate=1000 '!' h264parse '!' qtmux faststart=true '!' filesink "location=$($clip.Replace('\','/'))"
        if($LASTEXITCODE){ throw 'Synthetic sample generation failed' }
    } finally {
        $env:PATH=$savedPath; $env:GST_PLUGIN_SYSTEM_PATH_1_0=$savedPlugins
        $env:GST_PLUGIN_SCANNER_1_0=$savedScanner; $env:GST_REGISTRY=$savedRegistry
    }
}
if (!$BuildOnly) {
    $savedClip=$env:LAZYBUDDY_PROTOTYPE_CLIP
    try {
        $env:LAZYBUDDY_PROTOTYPE_CLIP=$clip
        Start-Process -FilePath (Join-Path $output 'build/release/lazybuddy-shell-prototype.exe') -WindowStyle Hidden
    } finally { $env:LAZYBUDDY_PROTOTYPE_CLIP=$savedClip }
}
