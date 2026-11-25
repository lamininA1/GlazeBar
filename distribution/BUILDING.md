# Build and package preview.8

The current release is **0.1.0-preview.8**, for Windows 11 x64 and ARM64.
It includes the Wallhaven browser and collections, readable comment colors and
modified GlazeWM 3.10.1-glazebar.2. See
[comment colors](../docs/comment-colors.md) and
[GlazeWM changes](../docs/glazewm-modifications.md).

## Build GlazeBar

Use Rust 1.93.0 or later, the matching MSVC C++ tools and Windows SDK.
Open a Visual Studio developer shell for the target architecture so `rc.exe`
and the correct linker/libraries are on PATH. On x64 Windows, use the x64-to-ARM64
developer shell for the ARM64 build. Runtime tests must run on the host architecture.

```powershell
rustup target add --toolchain 1.93.0 x86_64-pc-windows-msvc aarch64-pc-windows-msvc
cd native-rust
cargo +1.93.0 test --locked
cargo +1.93.0 build --locked --release --target x86_64-pc-windows-msvc
# Switch to the ARM64 developer environment before this command:
cargo +1.93.0 build --locked --release --target aarch64-pc-windows-msvc
```

The resulting `glazebar-native.exe` is installed as both `GlazeBar.exe`
and `GlazeTheme.exe`; the executable name selects its role.
The release PE version is defined in `native-rust/glazebar.rc`.

## Build the modified GlazeWM

Check out the modified GlazeWM source (upstream v3.10.1 plus
`glazewm-patches/glazewm-v3.10.1.patch`) with its vendored dependencies, then build
each architecture in its own developer shell with Rust nightly
(tested: `1.101.0-nightly`, 2026-10-18):

```powershell
$env:VERSION_NUMBER = '3.10.1-glazebar.2'
cargo +nightly test --offline --locked -p wm --bin glazewm
cargo +nightly build --release --offline --locked --target x86_64-pc-windows-msvc -p wm --bin glazewm -p wm-watcher --bin glazewm-watcher
# ARM64 developer environment:
cargo +nightly build --release --offline --locked --target aarch64-pc-windows-msvc -p wm --bin glazewm -p wm-watcher --bin glazewm-watcher
```

See [the WM notes](../docs/glazewm-modifications.md) for what the patch changes.

## Package each architecture

Commit the source and documentation first so the source revision and the tracked
source archive describe the release. `Repackage-Bundle.py` verifies the base bundle
manifest, the PE architecture of every binary, and the Cargo.lock files before it
reuses dependency vendors, licenses and the official GlazeWM MSI from the base bundle.
Use the previous release's verified ZIP for each architecture as the base.

```powershell
python distribution/Repackage-Bundle.py --base <previous-x64.zip> --bar native-rust/target/x86_64-pc-windows-msvc/release/glazebar-native.exe `
  --wm-runtime <glazewm-x64-release-folder> --wm-source <glazewm-checkout> --wm-version 3.10.1-glazebar.2 `
  --version 0.1.0-preview.7 --output distribution/output-preview7
python distribution/Repackage-Bundle.py --base <previous-arm64.zip> --bar native-rust/target/aarch64-pc-windows-msvc/release/glazebar-native.exe `
  --wm-runtime <glazewm-arm64-release-folder> --wm-source <glazewm-checkout> --wm-version 3.10.1-glazebar.2 `
  --version 0.1.0-preview.7 --output distribution/output-preview7
```

The `--wm-*` options are optional. Without them the modified GlazeWM from the base
bundle is kept. Each bundle includes the corresponding source and vendors in `source/`,
and build provenance in `source/BUILD-GLAZEBAR.json`. The bundled source config uses
`vendor/` for offline builds.

## Validate and build installers

Use PowerShell 7 and Inno Setup 6.7 or later. Run both isolated bundle tests:

```powershell
./distribution/Test-Bundle.ps1 -Bundle distribution/output-preview7/GlazeBar-Community-0.1.0-preview.7-windows-x64
./distribution/Test-Bundle.ps1 -Bundle distribution/output-preview7/GlazeBar-Community-0.1.0-preview.7-windows-arm64
./distribution/Build-Setup.ps1 -BundleRoot distribution/output-preview7 -Version 0.1.0-preview.7 -Architectures x64,arm64
./distribution/Test-SetupStartup.ps1 -SetupExe distribution/setup-output/GlazeBar-GlazeWM-Setup-0.1.0-preview.7-x64.exe
```

`Test-Bundle.ps1` checks integrity, isolated installation, Unicode/space paths,
backup/restore, preserved edits, corruption rejection, and partial-install rollback.
ARM64 package tests stage files without executing the ARM64 binary on x64.
`/CHECKSTARTUP` checks installer startup without installing the MSI or changing settings.

`-TestProfile` on `Build-Setup.ps1` creates isolated test installers. Never publish
those installers. The production source is copied to `setup-source` in the installation.
Publish both architecture ZIPs and setup EXEs, the corresponding source archives,
and SHA256SUMS.txt. The release is a prerelease; Windows binaries are unsigned.

## Hosted installer packaging

The Package release installers GitHub Actions workflow takes a draft release that
contains both bundle ZIPs and SHA256SUMS.txt. It verifies the archives, runs the
isolated package tests, compiles both installers on Windows, checks x64 installer
startup, and uploads the installers, merged checksums and validation record to the
draft. It does not publish the release or remove older releases.
