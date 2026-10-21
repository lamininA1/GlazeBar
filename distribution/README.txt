GlazeBar + GlazeWM community bundle 0.1.0-preview.6

For Windows 11 x64 / ARM64. Choose the package matching your Windows architecture.
Includes native Rust GlazeBar and modified GlazeWM 3.10.1-glazebar.1.
This is an independent community bundle, not an official GlazeWM release.

Current changes
Comment colors retain their muted hue while meeting the background contrast
threshold, instead of becoming identical to body text. Old palette caches are
regenerated. After updating, select your wallpaper/theme again and apply it to VS Code.
Also includes Wallhaven search filters, pagination, saved searches, persistent
previews and collections, plus layout-preserving Alt+M minimize/restore.

Install / update
1. Extract the entire ZIP.
2. Close GlazeWM and GlazeBar.
3. Uninstall an existing community bundle first, retaining your configuration backups.
4. Run in the extracted folder:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Install.ps1
   Add -AutoStart to start at sign-in.
5. Run %USERPROFILE%\.glzr\glazewm\wm\glazewm.exe.
For the bundled setup EXE, use GlazeBar + GlazeWM in the Start menu instead.

Install path: %USERPROFILE%\.glzr\glazewm
Backups: %USERPROFILE%\.glzr\community-backups
Existing files are backed up. Incompatible GlazeWM versions are not silently replaced.
If GlazeWM needs the Visual C++ runtime, the installer downloads it directly from
Microsoft, verifies its signature, and opens its installer. Internet access and
administrator approval may be required. GlazeBar does not need .NET, Go or WebView2.

Uninstall / restore
Close both programs, then run:
   powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\Uninstall.ps1
For an EXE installation, uninstall the community bundle in Windows Settings > Apps.
Original files are restored; files edited since installation are saved separately
under edits-at-uninstall. Backups, caches, the official GlazeWM installation,
and previously applied Windows colors/wallpaper are retained.

Shortcuts
Win+F1: shortcut guide
Win+Alt+Shift+B: show/hide bar
Win+Ctrl+Alt+Space: wallpaper picker
Alt+Shift+E: quit GlazeWM

Validation / limitations
x64 is checked on Windows. ARM64 is cross-built and package-checked; this version
has not been run on physical ARM64 hardware. Full MSI/UAC installation on a clean
Windows system is separate. Binaries are not code-signed.
Brightness controls depend on per-monitor WMI support; external DDC/CI is not included.
Weather defaults to Seoul, using Open-Meteo's free non-commercial service:
https://open-meteo.com/en/terms

License / source
GlazeBar and bundle scripts: GPL-3.0-only. GlazeWM and dependency licenses are retained.
Redistribute LICENSE, THIRD-PARTY-NOTICES.txt, licenses and source together.
Corresponding source, dependency source and build instructions are under source.
Full guide: https://github.com/lamininA1/GlazeBar
