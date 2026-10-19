#Requires -Version 5.1
[CmdletBinding()]
param([switch]$AutoStart, [switch]$StageOnly, [string]$ProfileRoot = $env:USERPROFILE)
$ErrorActionPreference = 'Stop'
if (!$StageOnly -and [IO.Path]::GetFullPath($ProfileRoot) -ne [IO.Path]::GetFullPath($env:USERPROFILE)) { throw 'Custom ProfileRoot is for StageOnly testing.' }
if ($StageOnly -and [IO.Path]::GetFullPath($ProfileRoot) -eq [IO.Path]::GetFullPath($env:USERPROFILE)) { throw 'StageOnly requires an isolated ProfileRoot.' }
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'manifest.json') -Raw | ConvertFrom-Json
foreach ($item in $manifest.files) {
    $file = Join-Path $PSScriptRoot $item.path
    if (!(Test-Path -LiteralPath $file) -or (Get-FileHash -LiteralPath $file -Algorithm SHA256).Hash -ne $item.sha256) { throw "Integrity check failed: $($item.path)" }
}
if (!$StageOnly) {
    $arch = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
    $expected = if ($manifest.architecture -eq 'arm64') { 'ARM64' } else { 'AMD64' }
    if ($arch -ne $expected) { throw "Use the package for your Windows architecture ($arch)." }
    if (Get-Process -Name glazewm,WorkspaceToggle -ErrorAction SilentlyContinue) { throw 'Exit GlazeWM and GlazeBar before installing. No running programs were stopped.' }
    if (![Environment]::Is64BitProcess) { throw 'Run this installer from 64-bit PowerShell.' }
    # The official GlazeWM MSI needs VC runtime even though GlazeBar links it statically.
    if (!(Test-Path -LiteralPath (Join-Path ([Environment]::SystemDirectory) 'vcruntime140.dll'))) {
        $download = Join-Path ([IO.Path]::GetTempPath()) ('glazebar-vcredist-' + [Guid]::NewGuid().ToString('N') + '.exe')
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        Invoke-WebRequest -UseBasicParsing "https://aka.ms/vc14/vc_redist.$($manifest.architecture).exe" -OutFile $download
        $signature = Get-AuthenticodeSignature -LiteralPath $download
        if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation') { throw 'Microsoft runtime signature verification failed.' }
        Write-Output 'GlazeWM requires the Microsoft VC runtime. Complete the Microsoft installer and its license dialog.'
        $runtime = Start-Process -FilePath $download -ArgumentList @('/install','/norestart') -Wait -PassThru
        if ($runtime.ExitCode -notin @(0,3010,1638)) { throw "Microsoft runtime installation failed or was cancelled: $($runtime.ExitCode)" }
    }
}
$target = Join-Path $ProfileRoot '.glzr\glazewm'
$stateFile = Join-Path $target 'community-install.json'
if (Test-Path -LiteralPath $stateFile) { throw 'Uninstall the existing community bundle before installing another version.' }
$backup = Join-Path $ProfileRoot ('.glzr\community-backups\' + [Guid]::NewGuid().ToString('N'))
$payload = Join-Path $PSScriptRoot 'payload'
$entries = @()
$shortcutPath = $null
$shortcutCreated = $false
try {
    foreach ($file in Get-ChildItem -LiteralPath $payload -Recurse -File) {
        $relative = $file.FullName.Substring($payload.Length + 1)
        $dest = Join-Path $target $relative
        $saved = Join-Path $backup $relative
        $existed = Test-Path -LiteralPath $dest
        if ($existed) { New-Item -ItemType Directory -Force (Split-Path $saved) | Out-Null; Copy-Item -LiteralPath $dest -Destination $saved }
        $entry = [ordered]@{ path = $relative; existed = $existed; sha256 = '' }
        $entries += $entry
        New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
        if ($relative -eq 'config.yaml') {
            $exe = (Join-Path $target 'WorkspaceToggle.exe').Replace("'", "''")
            $text = [IO.File]::ReadAllText($file.FullName).Replace('__GLAZEBAR_EXE__', $exe)
            [IO.File]::WriteAllText($dest, $text, (New-Object Text.UTF8Encoding $false))
        } else { Copy-Item -LiteralPath $file.FullName -Destination $dest }
        $entry.sha256 = (Get-FileHash -LiteralPath $dest).Hash
    }
    if (!$StageOnly) {
        $cli = Join-Path $env:ProgramFiles 'glzr.io\GlazeWM\cli\glazewm.exe'
        if (Test-Path -LiteralPath $cli) {
            $version = & $cli --version
            if ($LASTEXITCODE -ne 0 -or $version.Trim() -ne 'glazewm 3.10.1') { throw 'This bundle requires GlazeWM 3.10.1. Existing installation was not changed.' }
        } else {
            $msi = Join-Path $PSScriptRoot $manifest.glazewmMsi
            $process = Start-Process msiexec.exe -ArgumentList @('/i', ('"' + $msi + '"'), '/passive', '/norestart') -Wait -PassThru -WindowStyle Hidden
            if ($process.ExitCode -notin @(0,3010)) { throw "GlazeWM installation failed: $($process.ExitCode)" }
            if (!(Test-Path -LiteralPath $cli)) { throw 'GlazeWM CLI was not found after installation.' }
        }
        if ($AutoStart) {
            $shortcutPath = Join-Path ([Environment]::GetFolderPath('Startup')) 'GlazeBar Community.lnk'
            if (Test-Path -LiteralPath $shortcutPath) { throw 'Startup shortcut already exists; it was not overwritten.' }
            $link = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcutPath)
            $link.TargetPath = $cli
            $link.Arguments = 'start'
            $link.WorkingDirectory = Split-Path $cli
            $link.Save()
            $shortcutCreated = $true
        }
    }
    [ordered]@{ version = $manifest.version; backup = $backup; entries = $entries; startupShortcut = $shortcutPath; staged = [bool]$StageOnly } |
        ConvertTo-Json -Depth 8 | Set-Content -LiteralPath $stateFile -Encoding UTF8
    Write-Output "Installed to $target. Start GlazeWM from the Start menu. Automatic startup: $([bool]$AutoStart)."
} catch {
    foreach ($entry in $entries) {
        $dest = Join-Path $target $entry.path
        if ($entry.existed) { Copy-Item -LiteralPath (Join-Path $backup $entry.path) -Destination $dest -Force }
        elseif (Test-Path -LiteralPath $dest) { Remove-Item -LiteralPath $dest -Force }
    }
    if ($shortcutCreated -and (Test-Path -LiteralPath $shortcutPath)) { Remove-Item -LiteralPath $shortcutPath }
    throw
}
