param([Parameter(Mandatory=$true)][string]$Label)
$ErrorActionPreference = 'Stop'
$all = Get-CimInstance Win32_Process
$ids = [Collections.Generic.HashSet[int]]::new()
$all | Where-Object Name -eq 'zebar.exe' | ForEach-Object { [void]$ids.Add([int]$_.ProcessId) }
do {
    $added = $false
    foreach ($item in $all) {
        if ($ids.Contains([int]$item.ParentProcessId) -and $ids.Add([int]$item.ProcessId)) { $added = $true }
    }
} while ($added)
$before = @{}
foreach ($processId in $ids) {
    $p = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if ($p) { $before[$processId] = $p.CPU }
}
$clock = [Diagnostics.Stopwatch]::StartNew()
Start-Sleep -Seconds 8
$cpu = 0.0
$working = 0L
$private = 0L
$count = 0
$rows = foreach ($processId in $ids) {
    $p = Get-Process -Id $processId -ErrorAction SilentlyContinue
    if ($p) {
        $count++
        $working += $p.WorkingSet64
        $private += $p.PrivateMemorySize64
        if ($before.ContainsKey($processId)) { $cpu += $p.CPU - $before[$processId] }
        [pscustomobject]@{Name=$p.ProcessName; PID=$processId; WorkingMB=[math]::Round($p.WorkingSet64/1MB,1); PrivateMB=[math]::Round($p.PrivateMemorySize64/1MB,1)}
    }
}
$result = [pscustomobject]@{
    Label=$Label
    At=(Get-Date).ToString('o')
    Seconds=[math]::Round($clock.Elapsed.TotalSeconds,1)
    Processes=$count
    CPUPercent=[math]::Round(100*$cpu/$clock.Elapsed.TotalSeconds/[Environment]::ProcessorCount,3)
    WorkingSetSumMB=[math]::Round($working/1MB,1)
    PrivateMemoryMB=[math]::Round($private/1MB,1)
    Details=@($rows)
}
$result | ConvertTo-Json -Depth 5 | Set-Content -LiteralPath (Join-Path $PSScriptRoot ($Label+'.json')) -Encoding utf8
$result | Select-Object Label,Processes,CPUPercent,WorkingSetSumMB,PrivateMemoryMB | Format-List
