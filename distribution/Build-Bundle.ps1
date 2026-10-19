#Requires -Version 7.0
[CmdletBinding()]
param([switch]$SkipBuild, [string]$ReuseArchivesFrom)
$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot
$version = '0.1.0-preview.3'
$out = Join-Path $PSScriptRoot ('output-' + (Get-Date -Format 'yyyyMMdd-HHmmss'))
New-Item -ItemType Directory $out | Out-Null
if (!$SkipBuild) {
    Push-Location (Join-Path $repo 'native-rust')
    try {
        cargo fmt --check
        if ($LASTEXITCODE) { throw 'Formatting failed' }
        cargo test --locked
        if ($LASTEXITCODE) { throw 'Tests failed' }
        foreach ($target in @('aarch64-pc-windows-msvc','x86_64-pc-windows-msvc')) {
            cargo build --locked --release --target $target
            if ($LASTEXITCODE) { throw "Build failed: $target" }
        }
    } finally { Pop-Location }
}
function Zip-Tree($directory, $destination) {
    Write-Output "Creating archive: $destination"
    [IO.Compression.ZipFile]::CreateFromDirectory($directory, $destination, [IO.Compression.CompressionLevel]::Optimal, $false)
}
$common = Join-Path $out 'common'
$source = Join-Path $out 'source-stage'
New-Item -ItemType Directory -Force "$common/source", "$common/licenses", "$source/native-rust", "$source/native-rust/.cargo", "$source/distribution/templates" | Out-Null
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'standard-licenses') -Destination "$common/licenses/standard" -Recurse
Write-Output 'Preparing source and dependency archives...'
foreach ($name in @('src','fixtures','licenses','Cargo.toml','Cargo.lock','build.rs','glazebar.rc','LICENSE')) {
    Copy-Item -LiteralPath (Join-Path $repo "native-rust/$name") -Destination "$source/native-rust" -Recurse
}
if (!$ReuseArchivesFrom) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'vendor-glazebar') -Destination "$source/native-rust/vendor" -Recurse
}
[IO.File]::WriteAllText("$source/native-rust/.cargo/config.toml", [IO.File]::ReadAllText((Join-Path $repo 'native-rust/.cargo/config.toml')) + "`n[source.crates-io]`nreplace-with = 'vendored-sources'`n[source.vendored-sources]`ndirectory = 'vendor'`n")
Copy-Item -LiteralPath (Join-Path $repo 'icons') -Destination "$source/icons" -Recurse
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'templates/config.yaml'),(Join-Path $PSScriptRoot 'templates/cheatsheet.tsv') -Destination "$source/distribution/templates"
foreach ($file in @('Build-Bundle.ps1','Install.ps1','Uninstall.ps1','README.txt','README.ko.txt','Prepare-Upstream.ps1','Test-Bundle.ps1','Fetch-MissingLicenses.ps1','Fetch-StandardLicenses.ps1')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $file) -Destination "$source/distribution"
}
@'
GlazeBar source build (Windows, Rust 1.93+ and Visual Studio C++ tools):
cd native-rust
cargo test --offline --locked
cargo build --offline --locked --release --target aarch64-pc-windows-msvc
cargo build --offline --locked --release --target x86_64-pc-windows-msvc
Install the corresponding Rust target and MSVC/Windows SDK libraries first.
Copy the target's glazebar-native.exe as BOTH GlazeBar.exe and GlazeTheme.exe.
Runtime also needs icons/, config.yaml and cheatsheet.tsv; see distribution/.
Distribution scripts generate the install configuration and preserve prior files.
End-user install scripts support Windows PowerShell 5.1; bundle-building scripts
require PowerShell 7.
To rebuild the complete bundle: run distribution/Prepare-Upstream.ps1 followed
by distribution/Build-Bundle.ps1. This fetches the pinned GlazeWM release.
Fixture paths were anonymized; path-dependent MD5/SHA256 values recomputed.
Original GlazeBar code is GPL-3.0-only; third-party notices remain applicable.
'@ | Set-Content "$source/BUILD.txt" -Encoding utf8
if ($ReuseArchivesFrom) {
    # Only dependency trees are reused. Every first-party file is replaced below.
    Copy-Item -LiteralPath (Join-Path $ReuseArchivesFrom 'glazebar-source.zip') -Destination "$common/source/glazebar-source.zip"
    $zip = [IO.Compression.ZipFile]::Open("$common/source/glazebar-source.zip", [IO.Compression.ZipArchiveMode]::Update)
    try {
        $lockReader = [IO.StreamReader]::new($zip.GetEntry('native-rust/Cargo.lock').Open())
        try { $oldLock = $lockReader.ReadToEnd() } finally { $lockReader.Dispose() }
        if ($oldLock -cne [IO.File]::ReadAllText((Join-Path $repo 'native-rust/Cargo.lock'))) { throw 'Cannot reuse dependencies from a different Cargo.lock.' }
        foreach ($entry in @($zip.Entries)) {
            if (!$entry.FullName.StartsWith('native-rust/vendor/')) { $entry.Delete() }
        }
        foreach ($file in Get-ChildItem -LiteralPath $source -Recurse -File -Force) {
            $relative = $file.FullName.Substring($source.Length+1).Replace('\','/')
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,$file.FullName,$relative,[IO.Compression.CompressionLevel]::Optimal)
        }
    } finally { $zip.Dispose() }
} else { Zip-Tree $source "$common/source/glazebar-source.zip" }
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'upstream/glazewm-v3.10.1-source.zip') -Destination "$common/source"
if ($ReuseArchivesFrom) {
    Copy-Item -LiteralPath (Join-Path $ReuseArchivesFrom 'glazewm-vendor.zip') -Destination "$common/source/glazewm-vendor.zip"
} else { Zip-Tree (Join-Path $PSScriptRoot 'vendor-glazewm') "$common/source/glazewm-vendor.zip" }
@'
GlazeWM source is the unmodified v3.10.1 tag corresponding to the bundled MSI.
https://github.com/glzr-io/glazewm/releases/tag/v3.10.1
Extract glazewm-v3.10.1-source.zip; follow its CONTRIBUTING.md, rust-toolchain.toml
and .github/workflows for upstream build and MSI packaging instructions.
Dependency source is supplied in glazewm-vendor.zip. Extract it to a vendor
directory at the GlazeWM workspace root. To use it offline, add these source
sections to the workspace .cargo/config.toml, preserving its existing settings:
[source.crates-io]
replace-with = "vendored-sources"
[source.vendored-sources]
directory = "vendor"
Use cargo build --offline --locked with the appropriate Windows MSVC target.
Visual Studio tools, Windows SDK and the upstream packaging tools are required.
The official MSI is redistributed unchanged; its signing identity is upstream's.
'@ | Set-Content "$common/source/BUILD-GLAZEWM.txt" -Encoding utf8
$inventory = @()
foreach ($component in @('glazebar','glazewm')) {
    foreach ($crate in Get-ChildItem -LiteralPath (Join-Path $PSScriptRoot "vendor-$component") -Directory) {
        $toml = Get-Content -LiteralPath (Join-Path $crate.FullName 'Cargo.toml') -Raw
        $license = [regex]::Match($toml, '(?m)^license\s*=\s*"([^"]+)"').Groups[1].Value
        $licenseFile = [regex]::Match($toml, '(?m)^license-file\s*=\s*"([^"]+)"').Groups[1].Value
        $crateVersion = [regex]::Match($toml, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
        if (!$license -and !$licenseFile) { throw "Missing license declaration: $component/$($crate.Name)" }
        New-Item -ItemType Directory -Force "$common/licenses/$component/$($crate.Name)" | Out-Null
        Copy-Item -LiteralPath (Join-Path $crate.FullName 'Cargo.toml') -Destination "$common/licenses/$component/$($crate.Name)/PACKAGE-DECLARATION.toml"
        $documents = @(Get-ChildItem -LiteralPath $crate.FullName -Recurse -File | Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|AUTHORS|COPYRIGHT)' -or ($licenseFile -and $_.FullName -eq (Join-Path $crate.FullName $licenseFile)) })
        if ($documents.Count -eq 0) {
            $upstreamNotices = Join-Path $PSScriptRoot "upstream-notices/$component/$($crate.Name)"
            if (!(Test-Path -LiteralPath $upstreamNotices)) { throw "Missing license text: $component/$($crate.Name)" }
            New-Item -ItemType Directory -Force "$common/licenses/$component" | Out-Null
            Get-ChildItem -LiteralPath $upstreamNotices | Copy-Item -Destination "$common/licenses/$component/$($crate.Name)" -Recurse
        }
        foreach ($document in $documents) {
            $relative = $document.FullName.Substring($crate.FullName.Length + 1)
            $dest = Join-Path "$common/licenses/$component/$($crate.Name)" $relative
            New-Item -ItemType Directory -Force (Split-Path $dest) | Out-Null
            Copy-Item -LiteralPath $document.FullName -Destination $dest
        }
        $inventory += [ordered]@{ component=$component; name=$crate.Name; version=$crateVersion; license=$license; licenseFile=$licenseFile; bundledNotices=$documents.Count; recoveredNotices=($documents.Count -eq 0) }
    }
}
$inventory | ConvertTo-Json -Depth 5 | Set-Content "$common/licenses/dependencies.json" -Encoding utf8
Copy-Item -LiteralPath (Join-Path $repo 'native-rust/licenses/GO-LICENSE.txt'),(Join-Path $repo 'native-rust/licenses/AETHER-LICENSE.txt'),(Join-Path $repo 'icons/lucide/LICENSE'),(Join-Path $PSScriptRoot 'upstream/AETHER-UPSTREAM-README.md') -Destination "$common/licenses"
Rename-Item -LiteralPath "$common/licenses/LICENSE" -NewName 'LUCIDE-LICENSE.txt'
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'upstream/GPL-3.0.txt') -Destination "$common/LICENSE"
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'README.txt'),(Join-Path $PSScriptRoot 'README.ko.txt'),(Join-Path $PSScriptRoot 'Install.ps1'),(Join-Path $PSScriptRoot 'Uninstall.ps1') -Destination $common
@'
GlazeBar Community Bundle - third-party notices
Original GlazeBar code and bundle scripts: GPL-3.0-only.
GlazeWM 3.10.1: GNU GPL v3; https://github.com/glzr-io/glazewm
Go-derived sorting/Unicode code: BSD-3-Clause, The Go Authors; GO-LICENSE.txt.
Aether palette algorithms: MIT, Bjarne Overli and contributors.
https://github.com/omacom/aether/tree/29d07b1a8f474c77bf02aece948373f40abed105
The upstream README declares MIT but supplies no standalone license text;
AETHER-LICENSE.txt records that provenance and includes the MIT terms.
Lucide icons: ISC / Feather-derived icons MIT. See LUCIDE-LICENSE.txt.
https://github.com/lucide-icons/lucide/tree/d672fc43dbf60962d9d829e35bc3b3e5ed89a178
Rust dependency declarations and original notice texts: licenses/dependencies.json
and the component subdirectories. Full crate sources are in source/ archives.
The inventory includes dependencies for other platforms as well as Windows.
Where a crate omitted its license text, notices were retrieved from its pinned
upstream revision. Where upstream supplies only an SPDX declaration, that
declaration and author metadata are retained with supplemental standard terms.
These provenance cases are recorded under licenses/*/*/provenance.json.
Declarations and source notices govern each component; this inventory is not
a representation that all upstream projects underwent a legal audit.
Open-Meteo weather: https://open-meteo.com/ ; data licensed CC BY 4.0:
https://creativecommons.org/licenses/by/4.0/ . Display rounds temperature to C.
Wallhaven images are downloaded on request and are not included in this bundle.
All third-party copyrights and license terms remain with their respective owners.
'@ | Set-Content "$common/THIRD-PARTY-NOTICES.txt" -Encoding utf8
foreach ($arch in @('arm64','x64')) {
    $target = if ($arch -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $exe = Join-Path $repo "native-rust/target/$target/release/glazebar-native.exe"
    $bytes = [IO.File]::ReadAllBytes($exe)
    $machine = [BitConverter]::ToUInt16($bytes, [BitConverter]::ToInt32($bytes,60)+4)
    if ($machine -ne $(if ($arch -eq 'arm64') {0xAA64} else {0x8664})) { throw "Wrong binary architecture: $arch" }
    $folder = Join-Path $out "GlazeBar-Community-$version-windows-$arch"
    Copy-Item -LiteralPath $common -Destination $folder -Recurse
    New-Item -ItemType Directory -Force "$folder/payload", "$folder/upstream" | Out-Null
    Copy-Item -LiteralPath $exe -Destination "$folder/payload/GlazeBar.exe"
    Copy-Item -LiteralPath $exe -Destination "$folder/payload/GlazeTheme.exe"
    Copy-Item -LiteralPath (Join-Path $repo 'icons') -Destination "$folder/payload/icons" -Recurse
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'templates/config.yaml'),(Join-Path $PSScriptRoot 'templates/cheatsheet.tsv') -Destination "$folder/payload"
    $msi = "standalone-glazewm-v3.10.1-$arch.msi"
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot "upstream/$msi") -Destination "$folder/upstream"
    $files = @(Get-ChildItem -LiteralPath $folder -Recurse -File | ForEach-Object { [ordered]@{ path=$_.FullName.Substring($folder.Length+1); sha256=(Get-FileHash -LiteralPath $_.FullName).Hash } })
    [ordered]@{ version=$version; architecture=$arch; glazewmVersion='3.10.1'; glazewmMsi="upstream/$msi"; files=$files } | ConvertTo-Json -Depth 6 | Set-Content "$folder/manifest.json" -Encoding utf8
    & (Join-Path $PSScriptRoot 'Test-Bundle.ps1') -Bundle $folder
    Zip-Tree $folder "$folder.zip"
}
Get-ChildItem -LiteralPath $out -Filter '*.zip' | ForEach-Object { "$((Get-FileHash -LiteralPath $_.FullName).Hash.ToLower())  $($_.Name)" } | Set-Content "$out/SHA256SUMS.txt" -Encoding ascii
Write-Output "Bundle output: $out"
