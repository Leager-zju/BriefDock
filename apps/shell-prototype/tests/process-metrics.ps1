param([Parameter(Mandatory=$true)][int]$RootProcessId)
$ErrorActionPreference='Stop'
$all=Get-CimInstance Win32_Process | Select-Object ProcessId,ParentProcessId
$ids=[Collections.Generic.HashSet[int]]::new()
[void]$ids.Add($RootProcessId)
do {
    $added=$false
    foreach($item in $all){
        if($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)){$added=$true}
    }
} while($added)
$working=0L; $private=0L; $cpu=0.0; $count=0
foreach($processId in $ids){
    $process=Get-Process -Id $processId -ErrorAction SilentlyContinue
    if($process){$working+=$process.WorkingSet64; $private+=$process.PrivateMemorySize64; $cpu+=$process.TotalProcessorTime.TotalSeconds; $count++}
}
@{pid=$RootProcessId; processes=$count; working_set_sum_bytes=$working; private_bytes_sum=$private; cpu_seconds_sum=$cpu; sampled_at_ms=[DateTimeOffset]::UtcNow.ToUnixTimeMilliseconds()} | ConvertTo-Json -Compress
