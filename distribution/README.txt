preview.4 x64: GlazeWM 3.10.1-glazebar.1 (modified from https://github.com/glzr-io/glazewm).
Alt+M preserves the layout while minimizing/restoring real windows. Corrective placement is retained.
Use GlazeBar + GlazeWM in the Start menu, or run %USERPROFILE%\.glzr\glazewm\wm\glazewm.exe.
The unchanged upstream MSI is retained for CLI compatibility and rollback; community shortcuts run the modified build.
This release is x64 only. ARM64 stays on preview.3. No IME fix is claimed.

GlazeBar Community Bundle 0.1.0-preview.4 / GlazeWM 3.10.1

preview.3 hides Wi-Fi when no usable WLAN interface is available. Disconnected
Wi-Fi adapters remain visible. Popups dismiss on outside clicks, including the
bar, or foreground switches without requiring a prior click inside the popup.
Input controls and owned settings dialogs remain part of the popup.

Independent community bundle. Not an official GlazeWM release.
Windows 11 x64 (Intel / AMD).
GlazeBar needs no .NET, Go, Rust or WebView2 installation.
Modified GlazeWM is included alongside the unchanged official standalone MSI.
GlazeBar statically links its C runtime. The official GlazeWM binary still
requires Microsoft Visual C++ Runtime. If it is missing, Install.ps1 downloads
Microsoft's installer, verifies its signature, and shows its license/install
dialog. Internet access is required for this one-time prerequisite download.
Microsoft runtime terms apply separately; that installer is not redistributed
inside this ZIP. See https://aka.ms/vc14/vc_redist.x64.exe (x64) or
https://aka.ms/vc14/vc_redist.arm64.exe (ARM64).

INSTALL
1. Extract the entire ZIP to a local folder.
2. Exit any existing GlazeWM and GlazeBar processes.
3. In PowerShell, run:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
   Add -AutoStart to create a per-user startup shortcut.
4. Run %USERPROFILE%\.glzr\glazewm\wm\glazewm.exe (or the community setup shortcut).
The script backs up every existing file it replaces in
%USERPROFILE%\.glzr\community-backups, then installs GlazeBar and the
bundle configuration under %USERPROFILE%\.glzr\glazewm.
GlazeWM MSI may request administrator consent. Existing GlazeWM versions
other than 3.10.1 are left intact and installation is refused.
Existing third-party startup entries are not modified: avoid duplicate starts.

UNINSTALL / RESTORE
Exit GlazeWM and GlazeBar, then run:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Uninstall.ps1
Original files are restored. Files edited after installation are first copied
into the backup's edits-at-uninstall directory. New caches and user-created
files are retained. GlazeWM itself stays installed and can be removed using
Windows Settings > Installed apps. Wallpaper/Windows theme changes explicitly
applied while using the app are not undone by uninstalling the bundle.

FEATURES / LIMITS
Win+F1: shortcuts. Win+Alt+Shift+B: toggle bar.
Win+Ctrl+Alt+Space: wallpaper picker. Alt+Shift+E: exit GlazeWM.
Default weather: Seoul, using Open-Meteo; requires internet.
Weather data attribution: https://open-meteo.com/ (CC BY 4.0).
The free Open-Meteo endpoint is intended for non-commercial use; configure
an appropriate service plan/endpoint before commercial deployment.
Wallhaven search/download requires internet. No downloaded wallpapers are
redistributed; each image retains its own author's rights.
Power menu uses existing %USERPROFILE%\PowerProfiles\profiles.json if present.
Otherwise only available standard Windows power schemes can be selected;
unavailable schemes show a dash. No power schemes are created or overwritten.
Brightness is detected per display through WMI. Unsupported providers or
missing brightness classes disable the control without terminating the bar.
External-monitor DDC/CI control is not included. The resident process and
Windows file description are now GlazeBar (GlazeBar.exe).
The preview.4 x64 build is tested on physical x64 Windows. No preview.4 ARM64
build is provided. Clean-machine MSI/UAC validation remains pending.
Preview binaries/scripts are not signed. Checksums detect accidental changes;
they are not a substitute for publisher identity verification.

LICENSE / SOURCE
GlazeBar original code: GPL-3.0-only. See LICENSE and THIRD-PARTY-NOTICES.txt.
GlazeWM: GNU GPL v3; original notices are preserved in its source archive.
Other components retain their own licenses; see licenses/ and source/.
Complete GlazeBar source, Cargo.lock, vendored dependency sources, and build
instructions are in source/glazebar-source.zip. GlazeWM matching source and
dependency sources are also included. Keep these with any redistributed ZIP.
No warranty. Recipients retain the rights granted by the component licenses.
