#Requires -Version 7.0
param([Parameter(Mandatory)][string]$Bundle)
$ErrorActionPreference = 'Stop'
$Bundle = (Resolve-Path -LiteralPath $Bundle).Path
$bar = Join-Path $Bundle 'payload/GlazeBar.exe'
if (!(Test-Path -LiteralPath $bar)) { throw 'GlazeBar.exe payload missing' }
$versionInfo = [Diagnostics.FileVersionInfo]::GetVersionInfo($bar)
if ($versionInfo.FileDescription -ne 'GlazeBar' -or $versionInfo.ProductName -ne 'GlazeBar') { throw 'Task Manager product metadata missing' }
if (Test-Path -LiteralPath (Join-Path $Bundle 'payload/WorkspaceToggle.exe')) { throw 'Legacy resident executable still shipped' }
$profile = Join-Path $env:TEMP ("GlazeBar bundle's 한글 " + [Guid]::NewGuid().ToString('N'))
$target = Join-Path $profile '.glzr/glazewm'
New-Item -ItemType Directory -Force $target | Out-Null
$original = 'previous configuration - preserve these exact bytes'
[IO.File]::WriteAllText("$target/config.yaml", $original)
[IO.File]::WriteAllText("$target/unrelated.txt", 'untouched')
& "$Bundle/Install.ps1" -StageOnly -ProfileRoot $profile
$config = [IO.File]::ReadAllText("$target/config.yaml")
if ($config.Contains('__GLAZEBAR_EXE__') -or !$config.Contains((Join-Path $target 'GlazeBar.exe').Replace("'", "''"))) { throw 'Install path substitution failed' }
if ($config -match 'launch-vscode.lnk|code-server|shell:AppsFolder') { throw 'Personal configuration leaked' }
foreach ($file in Get-ChildItem -LiteralPath $Bundle -Filter '*.ps1') {
    $parseErrors = $null; $tokens = $null
    [void][Management.Automation.Language.Parser]::ParseFile($file.FullName, [ref]$tokens, [ref]$parseErrors)
    if ($parseErrors.Count) { throw "PowerShell parse failed: $($file.Name)" }
}
# A modified installed config must be saved before original bytes are restored.
[IO.File]::AppendAllText("$target/config.yaml", "`n# user edit after installation")
$state = Get-Content "$target/community-install.json" -Raw | ConvertFrom-Json
& "$Bundle/Uninstall.ps1" -StageOnly -ProfileRoot $profile
if ([IO.File]::ReadAllText("$target/config.yaml") -cne $original) { throw 'Original configuration was not restored' }
if (!(Test-Path -LiteralPath (Join-Path $state.backup 'edits-at-uninstall/config.yaml'))) { throw 'User edits were not preserved' }
if (Test-Path -LiteralPath "$target/GlazeBar.exe") { throw 'New executable not removed' }
if ([IO.File]::ReadAllText("$target/unrelated.txt") -cne 'untouched') { throw 'Unrelated file changed' }
# Tampering must be rejected before any payload writes.
$manifestPath = Join-Path $Bundle 'manifest.json'
$originalManifest = [IO.File]::ReadAllBytes($manifestPath)
try {
    $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
    $manifest.files[0].sha256 = '0' * 64
    $manifest | ConvertTo-Json -Depth 8 | Set-Content $manifestPath
    $rejected = $false
    try { & "$Bundle/Install.ps1" -StageOnly -ProfileRoot $profile } catch { $rejected = $_.Exception.Message -like 'Integrity check failed:*' }
    if (!$rejected) { throw 'Corrupt package was accepted' }
} finally { [IO.File]::WriteAllBytes($manifestPath, $originalManifest) }
$rollbackProfile = $profile + '-rollback'
$rollbackTarget = Join-Path $rollbackProfile '.glzr/glazewm'
New-Item -ItemType Directory -Force $rollbackTarget | Out-Null
[IO.File]::WriteAllText("$rollbackTarget/config.yaml", $original)
[IO.File]::WriteAllText("$rollbackTarget/icons", 'existing file blocks directory creation')
$failed = $false
try { & "$Bundle/Install.ps1" -StageOnly -ProfileRoot $rollbackProfile } catch { $failed = $true }
if (!$failed -or [IO.File]::ReadAllText("$rollbackTarget/config.yaml") -cne $original -or (Test-Path "$rollbackTarget/GlazeBar.exe") -or (Test-Path "$rollbackTarget/community-install.json")) { throw 'Partial installation rollback failed' }
if ([IO.File]::ReadAllText("$rollbackTarget/icons") -cne 'existing file blocks directory creation') { throw 'Rollback changed unrelated file' }
[ordered]@{ passed=$true; architecture=$manifest.architecture; isolatedProfile=$profile; checks=@('hash verification','path substitution with spaces and Unicode','backup and restore','preserve user edits','preserve unrelated files','remove new files','reject corruption','rollback partial install','script parsing') } | ConvertTo-Json | Set-Content (Join-Path $Bundle '..' "validation-$($manifest.architecture).json")
Write-Output "Bundle tests passed ($($manifest.architecture)). Scratch files retained: $profile"
