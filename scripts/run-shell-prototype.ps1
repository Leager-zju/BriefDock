param([switch]$BuildOnly,[string]$ServicePackages,[switch]$Release)
$ErrorActionPreference='Stop'
$projectRoot=Split-Path -Parent $PSScriptRoot
$prototype=Join-Path $projectRoot 'apps/shell-prototype'
$output=Join-Path $projectRoot 'target/qa-browser-video'
$profile=if($Release){'release'}else{'debug'}
$executable=Join-Path $output "build/$profile/lazybuddy-shell-prototype.exe"
if($ServicePackages){
    $ServicePackages=(Resolve-Path -LiteralPath $ServicePackages).Path
    foreach($id in @('hongguo','python-runtime')){
        $manifest=Get-Content -Raw -LiteralPath (Join-Path $ServicePackages "$id/module.json") | ConvertFrom-Json
        if($manifest.id -ne $id -or !(Test-Path -LiteralPath (Join-Path $ServicePackages "$id/$($manifest.entrypoint)"))){throw "Invalid service package: $id"}
    }
}
if(Get-Process -Name lazybuddy-shell-prototype -ErrorAction SilentlyContinue | Where-Object { $_.Path -eq $executable }) {throw 'Please exit this prototype from its tray menu before rebuilding it; X only hides the window.'}
$sdk=Join-Path $projectRoot 'target/native-player-sdk/PFiles64/gstreamer/1.0/msvc_x86_64'
New-Item -ItemType Directory -Path $output -Force | Out-Null
Push-Location $prototype
try {
    if (!(Test-Path node_modules/.bin/vite.cmd)) {
        npm.cmd ci --legacy-peer-deps --no-audit --no-fund
        if($LASTEXITCODE){ throw 'Prototype dependencies failed' }
    }
    Write-Host '[1/2] Building frontend...'
    npm.cmd run build
    if($LASTEXITCODE){ throw 'Prototype UI build failed' }
    $previousTarget=$env:CARGO_TARGET_DIR
    try {
        $env:CARGO_TARGET_DIR=Join-Path $output 'build'
        Write-Host '[2/2] Building desktop shell...'
        $buildArgs=@('build','--locked','--manifest-path','src-tauri/Cargo.toml')
        if($Release){$buildArgs+='--release'}
        & cargo @buildArgs
        if($LASTEXITCODE){ throw 'Prototype native build failed' }
    } finally { $env:CARGO_TARGET_DIR=$previousTarget }
} finally { Pop-Location }
$clip=Join-Path $output 'sample.mp4'
if (!$ServicePackages -and !(Test-Path -LiteralPath $clip)) {
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
    $savedPackages=$env:LAZYBUDDY_BROWSER_PACKAGES
    $savedFixture=$env:LAZYBUDDY_BROWSER_FIXTURE_FILE
    $savedProbe=$env:LAZYBUDDY_PROTOTYPE_PROBE
    try {
        $env:LAZYBUDDY_PROTOTYPE_CLIP=$clip
        $env:LAZYBUDDY_BROWSER_PACKAGES=$ServicePackages
        $env:LAZYBUDDY_BROWSER_FIXTURE_FILE=$null
        $env:LAZYBUDDY_PROTOTYPE_PROBE=$null
        Start-Process -FilePath $executable -NoNewWindow
        Write-Host 'Build complete. In LazyBuddy, select Hongguo and click Login.'
    } finally {
        $env:LAZYBUDDY_PROTOTYPE_CLIP=$savedClip;$env:LAZYBUDDY_BROWSER_PACKAGES=$savedPackages
        $env:LAZYBUDDY_BROWSER_FIXTURE_FILE=$savedFixture;$env:LAZYBUDDY_PROTOTYPE_PROBE=$savedProbe
    }
}
if($BuildOnly){Write-Host "Build complete: $executable"}
