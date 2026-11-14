param([switch]$BuildOnly)
$ErrorActionPreference = 'Stop'
$repoRoot = Split-Path -Parent $PSScriptRoot
$outputRoot = Join-Path $repoRoot 'target\debug'
$controller = Join-Path $outputRoot 'lazybuddy-cli.exe'
$application = Join-Path $outputRoot 'lazybuddy.exe'
$started = $null

function Invoke-HostCommand {
    param([string]$Command)
    $request = @{ version = 1; command = $Command; args = @{} } | ConvertTo-Json -Compress
    $info = New-Object System.Diagnostics.ProcessStartInfo
    $info.FileName = $controller
    $info.Arguments = 'host "' + $request.Replace('"', '\"') + '"'
    $info.UseShellExecute = $false
    $info.CreateNoWindow = $true
    $info.RedirectStandardOutput = $true
    $info.RedirectStandardError = $true
    $info.StandardOutputEncoding = [System.Text.UTF8Encoding]::new($false, $true)
    $process = New-Object System.Diagnostics.Process
    $process.StartInfo = $info
    try {
        if (-not $process.Start()) { throw 'Could not start the CLI.' }
        $stdout = $process.StandardOutput.ReadToEndAsync()
        $stderr = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(15000)) {
            $process.Kill()
            throw 'The CLI did not respond.'
        }
        $reply = $stdout.GetAwaiter().GetResult()
        $null = $stderr.GetAwaiter().GetResult()
        if ($process.ExitCode -ne 0) { return $null }
        try { return ($reply | ConvertFrom-Json) } catch { throw 'Invalid host response.' }
    } finally { $process.Dispose() }
}

try {
    Set-Location -LiteralPath $repoRoot
    $env:LAZYBUDDY_INSTANCE = 'default'
    Remove-Item Env:LAZYBUDDY_DATA_DIR -ErrorAction SilentlyContinue
    & cargo build --locked --offline -p lazybuddy-cli
    if ($LASTEXITCODE -ne 0) { throw 'CLI build failed.' }
    if (-not $BuildOnly) {
        $status = Invoke-HostCommand 'status'
        if ($status -and $status.ok) {
            $allowed = @($application, (Join-Path $repoRoot 'target\release\lazybuddy.exe'))
            if ($status.data.exe -notin $allowed) { throw 'A LazyBuddy instance from another directory is running; it was not stopped.' }
            $existing = Get-Process -Id $status.data.pid -ErrorAction Stop
            if ($existing.Path -ne $status.data.exe) { throw 'Could not verify the running instance.' }
            $reply = Invoke-HostCommand 'app.quit'
            if (-not $reply.ok -or -not $existing.WaitForExit(20000)) { throw 'The existing host did not finish shutdown.' }
        }
    }
    Write-Host 'Building the LazyBuddy base host (no media SDK required)...'
    & cargo build --locked --offline -p lazybuddy
    if ($LASTEXITCODE -ne 0) { throw 'Host build failed; no old binary will be launched.' }
    if ($BuildOnly) { Write-Host 'LazyBuddy build complete.'; exit 0 }
    $started = Start-Process -FilePath $application -ArgumentList '--background' -WorkingDirectory $repoRoot -WindowStyle Hidden -PassThru
    $deadline = (Get-Date).AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 250
        if ($started.HasExited) { throw 'LazyBuddy exited during startup.' }
        $status = Invoke-HostCommand 'status'
        if ($status.ok -and $status.data.pid -eq $started.Id) { break }
    } while ((Get-Date) -lt $deadline)
    if (-not $status.ok -or $status.data.pid -ne $started.Id) { throw 'LazyBuddy did not become ready.' }
    if (-not (Invoke-HostCommand 'window.show').ok) { throw 'Could not display LazyBuddy.' }
    Write-Host 'LazyBuddy started. Install optional features from the app.'
    exit 0
} catch {
    if ($started -and -not $started.HasExited) { $started.Kill() }
    Write-Host ('ERROR: ' + $_.Exception.Message) -ForegroundColor Red
    exit 1
}
