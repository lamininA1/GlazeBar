#Requires -Version 5.1
[CmdletBinding()]
param([switch]$StageOnly, [string]$ProfileRoot = $env:USERPROFILE)
$ErrorActionPreference = 'Stop'
if (!$StageOnly -and [IO.Path]::GetFullPath($ProfileRoot) -ne [IO.Path]::GetFullPath($env:USERPROFILE)) { throw 'Custom ProfileRoot is for StageOnly testing.' }
if ($StageOnly -and [IO.Path]::GetFullPath($ProfileRoot) -eq [IO.Path]::GetFullPath($env:USERPROFILE)) { throw 'StageOnly requires an isolated ProfileRoot.' }
if (!$StageOnly -and (Get-Process -Name glazewm,WorkspaceToggle -ErrorAction SilentlyContinue)) { throw 'Exit GlazeWM and GlazeBar before uninstalling.' }
$target = [IO.Path]::GetFullPath((Join-Path $ProfileRoot '.glzr\glazewm'))
$stateFile = Join-Path $target 'community-install.json'
$state = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
if ([bool]$state.staged -ne [bool]$StageOnly) { throw 'Installation mode does not match.' }
$backupRoot = [IO.Path]::GetFullPath((Join-Path $ProfileRoot '.glzr\community-backups')) + '\'
$backup = [IO.Path]::GetFullPath($state.backup)
if (!$backup.StartsWith($backupRoot, [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid backup location.' }
$recovery = Join-Path $backup 'edits-at-uninstall'
# Validate every path and backup before changing anything.
foreach ($entry in $state.entries) {
    $dest = [IO.Path]::GetFullPath((Join-Path $target $entry.path))
    if (!$dest.StartsWith($target + '\', [StringComparison]::OrdinalIgnoreCase)) { throw 'Invalid manifest path.' }
    if ($entry.existed -and !(Test-Path -LiteralPath (Join-Path $backup $entry.path))) { throw "Missing backup: $($entry.path)" }
}
foreach ($entry in $state.entries) {
    $dest = Join-Path $target $entry.path
    if ((Test-Path -LiteralPath $dest) -and (Get-FileHash -LiteralPath $dest).Hash -ne $entry.sha256) {
        $saved = Join-Path $recovery $entry.path
        New-Item -ItemType Directory -Force (Split-Path $saved) | Out-Null
        Copy-Item -LiteralPath $dest -Destination $saved -Force
    }
    if ($entry.existed) {
        New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
        Copy-Item -LiteralPath (Join-Path $backup $entry.path) -Destination $dest -Force
    }
    elseif (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Force }
}
if (!$StageOnly -and $state.startupShortcut) {
    $allowed = Join-Path ([Environment]::GetFolderPath('Startup')) 'GlazeBar Community.lnk'
    if ($state.startupShortcut -eq $allowed -and (Test-Path -LiteralPath $allowed)) { Remove-Item -LiteralPath $allowed }
}
Remove-Item -LiteralPath $stateFile
Write-Output "Previous files restored. Backups and later edits retained at $backup. GlazeWM remains installed; remove it through Windows Installed apps if desired."
