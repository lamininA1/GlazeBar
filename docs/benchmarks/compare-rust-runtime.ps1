$ErrorActionPreference='Stop'
$exe=Join-Path $PSScriptRoot 'WorkspaceToggle.exe'
$theme=Join-Path $PSScriptRoot 'GlazeTheme.exe'
$installed=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rust-installation.json') -Raw | ConvertFrom-Json
$go=Join-Path $installed.backup 'WorkspaceToggle.exe'
$goTheme=Join-Path $installed.backup 'GlazeTheme.exe'
$legacy=Join-Path $PSScriptRoot 'backup-before-go-20261019\WorkspaceToggle.exe'
$output=Join-Path $PSScriptRoot ('rust-runtime-comparison-'+(Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory -Path $output | Out-Null
$saved=Join-Path $output 'WorkspaceToggle.rust.exe'
$savedTheme=Join-Path $output 'GlazeTheme.rust.exe'
Copy-Item -LiteralPath $exe -Destination $saved
Copy-Item -LiteralPath $theme -Destination $savedTheme
$hash=(Get-FileHash -LiteralPath $saved).Hash
if ($hash -ne $installed.sha256) {throw 'Installed executable differs from Rust installation record'}
foreach ($path in @($go,$goTheme,$legacy)) {if (-not (Test-Path -LiteralPath $path -PathType Leaf)) {throw "Missing comparison input: $path"}}
if (-not ('RustRuntimeResources' -as [type])) {
Add-Type 'using System;using System.Runtime.InteropServices;public static class RustRuntimeResources{[DllImport("user32.dll")]public static extern uint GetGuiResources(IntPtr process,uint flags);}'
}
function Stop-ComparisonBar {
    $running=@(Get-Process WorkspaceToggle -ErrorAction SilentlyContinue | Where-Object {$_.Path -eq $exe})
    if (-not $running.Count) {return}
    $null=Start-Process -FilePath $exe -ArgumentList '--stop' -WindowStyle Hidden -Wait
    foreach ($p in $running) {if (-not $p.WaitForExit(15000)) {throw 'Resident did not stop; binaries were not replaced'}}
}
function Get-ComparisonSample([int]$processId) {
    $p=Get-Process -Id $processId
    [pscustomobject]@{utc=[DateTime]::UtcNow.ToString('o');pid=$p.Id;workingMiB=$p.WorkingSet64/1MB;privateMiB=$p.PrivateMemorySize64/1MB;cpuSeconds=$p.TotalProcessorTime.TotalSeconds;handles=$p.HandleCount;threads=$p.Threads.Count;gdiObjects=[RustRuntimeResources]::GetGuiResources($p.Handle,0);userObjects=[RustRuntimeResources]::GetGuiResources($p.Handle,1)}
}
$results=@()
try {
    foreach ($variant in @('WinForms','Go','Rust')) {
        Stop-ComparisonBar
        $source=switch ($variant) {'WinForms' {$legacy} 'Go' {$go} 'Rust' {$saved}}
        $engine=if ($variant -eq 'Rust') {$savedTheme} else {$goTheme}
        Copy-Item -LiteralPath $source -Destination $exe -Force
        Copy-Item -LiteralPath $engine -Destination $theme -Force
        $p=Start-Process -FilePath $exe -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -PassThru
        Write-Output "$variant PID $($p.Id): warmup 30 seconds."
        Start-Sleep -Seconds 30
        Write-Output "${variant}: sampling 60 seconds."
        $samples=@()
        for ($i=0;$i -le 12;$i++) {
            $samples+=Get-ComparisonSample $p.Id
            if ($i -lt 12) {Start-Sleep -Seconds 5}
        }
        $elapsed=([DateTime]$samples[-1].utc-[DateTime]$samples[0].utc).TotalSeconds
        $cpu=$samples[-1].cpuSeconds-$samples[0].cpuSeconds
        $result=[ordered]@{variant=$variant;sha256=(Get-FileHash -LiteralPath $exe).Hash;warmupSeconds=30;elapsedSeconds=$elapsed;logicalProcessors=[Environment]::ProcessorCount;cpuDeltaSeconds=$cpu;cpuPercentMachine=100*$cpu/$elapsed/[Environment]::ProcessorCount;workingMiBMean=($samples | Measure-Object workingMiB -Average).Average;privateMiBMean=($samples | Measure-Object privateMiB -Average).Average;samples=$samples}
        $results+=$result
        $result | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output "$variant.json") -Encoding utf8
        Write-Output ("{0}: working {1:N2} MiB, private {2:N2} MiB, CPU time {3:N4}s" -f $variant,$result.workingMiBMean,$result.privateMiBMean,$cpu)
    }
} finally {
    Stop-ComparisonBar
    Copy-Item -LiteralPath $saved -Destination $exe -Force
    Copy-Item -LiteralPath $savedTheme -Destination $theme -Force
    $restored=Start-Process -FilePath $exe -WindowStyle Hidden -PassThru
    $deadline=[DateTime]::UtcNow.AddSeconds(15)
    $healthy=$false
    do {
        if ($restored.HasExited) {break}
        try {$ready=Get-Content -LiteralPath (Join-Path $PSScriptRoot 'rust-ready.json') -Raw | ConvertFrom-Json;if ($ready.pid -eq $restored.Id -and $ready.monitors -gt 0) {$healthy=$true;break}} catch { }
        Start-Sleep -Milliseconds 100
    } while ([DateTime]::UtcNow -lt $deadline)
    $results | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath (Join-Path $output 'comparison.json') -Encoding utf8
    if (-not $healthy) {throw "Rust restart did not become ready; recovery binaries are in $output"}
    Write-Output "Rust restored as PID $($restored.Id). Results: $output"
}
