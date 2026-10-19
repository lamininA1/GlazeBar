#Requires -Version 7.0
[CmdletBinding()]
param([Parameter(Mandatory)][string]$SetupExe)
$ErrorActionPreference = 'Stop'
$SetupExe = (Resolve-Path -LiteralPath $SetupExe).Path
$log = Join-Path ([IO.Path]::GetTempPath()) ('GlazeBar-startup-' + [guid]::NewGuid().ToString('N') + '.log')
$process = Start-Process -FilePath $SetupExe -ArgumentList @('/CHECKSTARTUP', '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', ('/LOG="' + $log + '"')) -WindowStyle Hidden -PassThru
if (!$process.WaitForExit(30000)) {
    $process.Kill()
    throw "Setup startup check timed out. Log: $log"
}
# InitializeSetup deliberately returns False after logging success, so no
# payload extraction, installation, or settings changes can take place.
$expected = 'GlazeBar startup check passed: ' + (Join-Path $env:USERPROFILE '.glzr\glazewm\community-install.json')
if (!(Test-Path -LiteralPath $log)) { throw "Setup did not create a startup log: $SetupExe" }
$text = Get-Content -LiteralPath $log -Raw
if ($process.ExitCode -ne 1 -or !$text.Contains($expected) -or $text -match 'Unknown constant|Runtime error') {
    throw "Setup startup check failed (exit $($process.ExitCode)). Log: $log"
}
Write-Output "PASS: production profile resolution; no installation performed. Log: $log"
