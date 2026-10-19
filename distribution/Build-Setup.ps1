#Requires -Version 7.0
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$BundleRoot,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'setup-output'),
    [string]$Compiler = (Join-Path $env:LOCALAPPDATA 'Programs/Inno Setup 6/ISCC.exe'),
    [string]$TestProfile
)
$ErrorActionPreference = 'Stop'
if (!(Test-Path -LiteralPath $Compiler)) { throw 'Install Inno Setup 6.7+ or pass -Compiler with the path to ISCC.exe.' }
$BundleRoot = (Resolve-Path -LiteralPath $BundleRoot).Path
$OutputDirectory = [IO.Path]::GetFullPath($OutputDirectory)
New-Item -ItemType Directory -Force $OutputDirectory | Out-Null
$architectures = if ($TestProfile) { @('arm64') } else { @('arm64','x64') }
foreach ($arch in $architectures) {
    $bundle = Join-Path $BundleRoot "GlazeBar-Community-0.1.0-preview.3-windows-$arch"
    $manifest = Get-Content -LiteralPath (Join-Path $bundle 'manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.architecture -ne $arch) { throw "Incorrect bundle architecture: $arch" }
    foreach ($file in $manifest.files) {
        if ((Get-FileHash -LiteralPath (Join-Path $bundle $file.path)).Hash -ne $file.sha256) { throw "Bundle integrity failed: $($file.path)" }
    }
    $arguments = @('/Q', "/DBundleDir=$bundle", "/DOutputDir=$OutputDirectory", "/DArch=$arch")
    if ($TestProfile) { $arguments += "/DTestProfile=$([IO.Path]::GetFullPath($TestProfile))" }
    & $Compiler @arguments (Join-Path $PSScriptRoot 'GlazeBar-Setup.iss')
    if ($LASTEXITCODE -ne 0) { throw "Installer compilation failed: $arch" }
}
Get-ChildItem -LiteralPath $OutputDirectory -Filter '*.exe' | ForEach-Object {
    "$((Get-FileHash -LiteralPath $_.FullName).Hash.ToLower())  $($_.Name)"
} | Set-Content -LiteralPath (Join-Path $OutputDirectory 'SHA256SUMS.txt') -Encoding ascii
Write-Output "Setup files: $OutputDirectory"
