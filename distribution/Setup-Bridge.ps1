#Requires -Version 5.1
param(
    [Parameter(Mandatory)][ValidateSet('Install','Uninstall','CheckUninstall')][string]$Action,
    [Parameter(Mandatory)][string]$Bundle,
    [switch]$AutoStart,
    [string]$TestProfile
)
$ErrorActionPreference = 'Stop'
# A parent PowerShell 7 session can export incompatible module paths.
$env:PSModulePath = Join-Path $PSHOME 'Modules'
Import-Module Microsoft.PowerShell.Utility,Microsoft.PowerShell.Management,Microsoft.PowerShell.Security -ErrorAction Stop
$logRoot = Join-Path $env:LOCALAPPDATA 'GlazeBarSetup'
if ($TestProfile) { $logRoot = Join-Path $TestProfile '.setup-logs' }
New-Item -ItemType Directory -Force $logRoot | Out-Null
$logAction = if ($Action -eq 'CheckUninstall') { 'uninstall' } else { $Action.ToLower() }
$log = Join-Path $logRoot ($logAction + '.log')
try {
    if ($Action -eq 'CheckUninstall') {
        if (!$TestProfile -and (Get-Process -Name glazewm,WorkspaceToggle -ErrorAction SilentlyContinue)) { throw 'Close GlazeWM and GlazeBar before uninstalling.' }
        $profile = if ($TestProfile) { $TestProfile } else { $env:USERPROFILE }
        $state = Get-Content -LiteralPath (Join-Path $profile '.glzr/glazewm/community-install.json') -Raw | ConvertFrom-Json
        foreach ($entry in $state.entries) {
            if ($entry.existed -and !(Test-Path -LiteralPath (Join-Path $state.backup $entry.path))) { throw "Backup missing: $($entry.path)" }
        }
        exit 0
    }
    $arguments = @{}
    if ($TestProfile) { $arguments.StageOnly = $true; $arguments.ProfileRoot = $TestProfile }
    if ($Action -eq 'Install' -and $AutoStart -and !$TestProfile) { $arguments.AutoStart = $true }
    & (Join-Path $Bundle ($Action + '.ps1')) @arguments *>&1 | Out-File -LiteralPath $log -Encoding UTF8
    exit 0
} catch {
    $_ | Out-String | Add-Content -LiteralPath $log -Encoding UTF8
    exit 1
}
