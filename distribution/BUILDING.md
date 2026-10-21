# Build and package preview.6

The current release is **0.1.0-preview.6**, for Windows 11 x64 and ARM64.
It includes the readable comment-color fix, Wallhaven browser and collections,
and modified GlazeWM 3.10.1-glazebar.1. See
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
Cross-building does not establish that the ARM64 app runs correctly on hardware.

## Package each architecture

Commit the source and documentation before repackaging so the source revision
and tracked source archive describe the release. `Repackage-Bundle.py` verifies
the base bundle manifest, executable PE architecture, and Cargo.lock before
reusing dependency vendors, licenses, official GlazeWM MSI, and modified WM.
Use a locally retained, verified preview.5 ZIP for each architecture; these
base archives need not remain published after preview.6 is validated.

```powershell
python distribution/Repackage-Bundle.py --base <verified-x64-base.zip> --bar native-rust/target/x86_64-pc-windows-msvc/release/glazebar-native.exe --version 0.1.0-preview.6 --output distribution/output-preview6
python distribution/Repackage-Bundle.py --base <verified-arm64-base.zip> --bar native-rust/target/aarch64-pc-windows-msvc/release/glazebar-native.exe --version 0.1.0-preview.6 --output distribution/output-preview6
```

Each bundle includes corresponding source and vendors in
`source/glazebar-source.zip`, updated package documentation, and build provenance
in `source/BUILD-GLAZEBAR.json`. The bundled source config uses `vendor/` for
offline builds. The unchanged GlazeWM sources and vendors remain included.

For a fresh modified WM build, follow
[the WM build notes](../docs/glazewm-modifications.md) and
`Build-ModifiedBundle.py`; the older `Build-Bundle.ps1` creates upstream-based
bundles and is not the current modified-WM release recipe.

## Validate and build installers

Use PowerShell 7 and Inno Setup 6.7 or later. Run both isolated bundle tests:

```powershell
./distribution/Test-Bundle.ps1 -Bundle distribution/output-preview6/GlazeBar-Community-0.1.0-preview.6-windows-x64
./distribution/Test-Bundle.ps1 -Bundle distribution/output-preview6/GlazeBar-Community-0.1.0-preview.6-windows-arm64
./distribution/Build-Setup.ps1 -BundleRoot distribution/output-preview6 -Version 0.1.0-preview.6 -Architectures x64,arm64
./distribution/Test-SetupStartup.ps1 -SetupExe distribution/setup-output/GlazeBar-GlazeWM-Setup-0.1.0-preview.6-x64.exe
```

`Test-Bundle.ps1` checks integrity, isolated installation, Unicode/space paths,
backup/restore, preserved edits, corruption rejection, and partial-install rollback.
ARM64 package tests stage files without executing the ARM64 binary on x64.
Run ARM64 startup and app checks on an actual ARM64 machine separately.
`/CHECKSTARTUP` checks installer startup without installing MSI or changing settings;
it does not replace full clean-machine MSI/UAC installation testing.

`-TestProfile` on `Build-Setup.ps1` creates isolated test installers. Never publish
those installers. The production source is copied to `setup-source` in the installation.
Publish both architecture ZIPs and setup EXEs, corresponding source archives,
and SHA256SUMS.txt. The release is a prerelease; Windows binaries are unsigned.

After updating an installed bundle, select the wallpaper/theme again and apply
it to regenerate palettes and exported comment colors. Existing community
bundles must be uninstalled first; keep the user-settings backups.

## Hosted installer packaging

The Package release installers GitHub Actions workflow accepts a draft release
containing both bundle ZIPs and SHA256SUMS.txt. It verifies the archives, runs
isolated package tests, compiles both installers on Windows, checks x64 installer
startup, and uploads installers, merged checksums and validation to the draft.
The workflow does not publish the release or remove older releases.
