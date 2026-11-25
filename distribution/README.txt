GlazeBar + GlazeWM community bundle 0.1.0-preview.8

For Windows 11 x64 / ARM64. Choose the package matching your Windows architecture.
Includes native Rust GlazeBar and modified GlazeWM 3.10.1-glazebar.2.
This is an independent community bundle, not an official GlazeWM release.

Current changes
Fixes false Remote Desktop auto-pause triggers caused by WSL2/WSLg's internal
msrdc.exe or hidden windows, and adds default ignore rules for Windows Task Manager
and Hancom Office helper windows.

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

Notes
The ARM64 package is cross-built on x64. Binaries are not code-signed.
Brightness controls depend on per-monitor WMI support; external DDC/CI is not included.
Weather defaults to Seoul, using Open-Meteo's free non-commercial service:
https://open-meteo.com/en/terms

License / source
GlazeBar and bundle scripts: GPL-3.0-only. GlazeWM and dependency licenses are retained.
Redistribute LICENSE, THIRD-PARTY-NOTICES.txt, licenses and source together.
Corresponding source, dependency source and build instructions are under source.
Full guide: https://github.com/lamininA1/GlazeBar
