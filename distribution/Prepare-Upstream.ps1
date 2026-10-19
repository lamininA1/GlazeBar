#Requires -Version 7.0
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
$upstream = Join-Path $PSScriptRoot 'upstream'
New-Item -ItemType Directory -Force $upstream | Out-Null
$release = Invoke-RestMethod https://api.github.com/repos/glzr-io/glazewm/releases/tags/v3.10.1
$release | ConvertTo-Json -Depth 20 | Set-Content "$upstream/glazewm-release.json"
$expected = @{
    'arm64' = '1dbd1e28ade652cddb924773fc6f42ac0beef0520f0b45e7b3ec100f9ed58e79'
    'x64' = 'ad4122dd4420690e502e7df3ce8d1e4772bf1246cc815db0dd2aaac0e4dc583f'
}
foreach ($arch in @('arm64','x64')) {
    $name = "standalone-glazewm-v3.10.1-$arch.msi"
    $asset = $release.assets | Where-Object name -EQ $name
    if (!$asset) { throw "Missing release asset: $name" }
    Invoke-WebRequest $asset.browser_download_url -OutFile "$upstream/$name"
    if ((Get-FileHash "$upstream/$name").Hash.ToLower() -ne $expected[$arch]) { throw "Checksum mismatch: $name" }
}
Invoke-WebRequest https://github.com/glzr-io/glazewm/archive/refs/tags/v3.10.1.zip -OutFile "$upstream/glazewm-v3.10.1-source.zip"
Invoke-WebRequest https://raw.githubusercontent.com/glzr-io/glazewm/v3.10.1/LICENSE.md -OutFile "$upstream/GPL-3.0.txt"
Invoke-WebRequest https://raw.githubusercontent.com/omacom/aether/29d07b1a8f474c77bf02aece948373f40abed105/README.md -OutFile "$upstream/AETHER-UPSTREAM-README.md"
Expand-Archive "$upstream/glazewm-v3.10.1-source.zip" "$upstream/source" -Force
Push-Location $repo
try {
    cargo vendor --locked --manifest-path native-rust/Cargo.toml distribution/vendor-glazebar > distribution/vendor-config.toml
    if ($LASTEXITCODE) { throw 'GlazeBar dependency vendoring failed' }
    cargo +stable vendor --locked --manifest-path distribution/upstream/source/glazewm-3.10.1/Cargo.toml distribution/vendor-glazewm > distribution/glazewm-vendor-config.toml
    if ($LASTEXITCODE) { throw 'GlazeWM dependency vendoring failed' }
} finally { Pop-Location }
& (Join-Path $PSScriptRoot 'Fetch-MissingLicenses.ps1')
& (Join-Path $PSScriptRoot 'Fetch-StandardLicenses.ps1')
