# GlazeWM modifications — preview.4 x64

This is a modified build of [glzr-io/glazewm](https://github.com/glzr-io/glazewm), based on v3.10.1, not an upstream release. GlazeWM retains its upstream GPL-3.0 license. GlazeBar is a separate component.

The v0.1.0-preview.4 x64 bundle includes these modifications. ARM64 remains on preview.3 with the original GlazeWM:

- `wm-toggle-minimized-group` / `wm-restore-minimized-group`: real native minimization, with the original workspace, split identifiers, directions, proportions and focus order captured by the WM. No workspace switching workaround.
- `wm-redraw`: remove empty nested splits and return their space to surviving siblings. Also clean empty ancestors when detaching windows.
- Avoid sending synthetic foreground input when the target window already owns foreground focus.
- Compare actual OS focus with WM focus and observe foreground events without a synchronous activation wait. The earlier 100 ms `WM_NULL` acknowledgement was removed during performance investigation. The user isolated the Korean IME symptom to an existing Codex session. This is not a confirmed IME fix.
- GlazeBar calls the new group command on the modified WM, retaining legacy compatibility with upstream builds. Suppress automatic split changes for two seconds during native minimize/restore transitions; this does not delay the restore command.
- Restore window types directly into the saved tree, skipping ordinary per-window insertion heuristics. Restore to the final rectangle instead of issuing an initial `SW_RESTORE` followed by another placement. Skip redundant shell cloaking calls when visibility already matches, and apply focus after group restoration.
- Group placement uses `SW_SHOWNOACTIVATE` to avoid activating every restored window; focus is assigned once afterwards. Late minimize notifications from rapid toggles are handled during a bounded restore interval.
- Retain the final screen-coordinate `SetWindowPos` correction after restoring. Removing this correction caused native window overlap, so that experiment was reverted at the user's request. Layout correctness takes priority over further restore-speed changes.
- Fix an upstream corner-position unit-test fixture: a 600-pixel window with one pixel visible at the left edge starts at -599, not +1.

The snapshot is in memory for the running WM session. Closed or moved windows are excluded on restore; newly opened windows remain managed. Crash/restart recovery of the saved tree is not implemented.

Validation on 2026-10-19:

- WM unit tests: 4 passed, including tree restoration and nested empty-split removal.
- GlazeBar unit tests: 76 passed.
- Initial WM build: 7 real windows minimized/restored three times, verifying native `IsIconic`, unchanged displayed workspaces, split IDs, proportions and geometry. Restore measured 545–623 ms including test/CLI overhead.
- Five rapid double toggles preserved the tree with the bar stopped.
- Alt+M integration exposed automatic split changes during transient focus events; suppression was built, installed, and passed live Alt+M tests after unlocking.
- Performance follow-up: initial 8-window Alt+M restore was 758–780 ms. With redundant restoration/cloaking work removed, a later 6-window test measured 352–469 ms restore and 58–91 ms minimize, preserving both the WM tree and native window rectangles. Different window counts mean this is not a controlled speedup comparison. Direct WM command responses measured 1–14 ms; that excludes completion of asynchronous native window transitions.
- Release scope: the verified x64 corrective-placement build. No new ARM64 build is included.
- Corrective-placement rollback verified with 8 windows: native window rectangles and saved layout preserved, workspaces unchanged. This final Alt+M check measured 826 ms restore; it does not establish a 500 ms guarantee. Further performance experiments were stopped at the user's request.
- Follow-up checks found that disabling activation per window alone did not resolve the remaining delay (8-window restore still 733–823 ms). Minimize/maximize animations were already disabled, so changing that setting did not help; the original setting was restored after the check. Do not attribute the remaining delay to animation without evidence.

The upstream-based patch is [glazewm-v3.10.1.patch](../glazewm-patches/glazewm-v3.10.1.patch). Build with Rust nightly (tested: `1.101.0-nightly`, `b6eeb93c9`, 2026-10-18), MSVC x64, and `VERSION_NUMBER=3.10.1-glazebar.1`, then `cargo build --release --locked -p wm --bin glazewm`. Apply the patch to the v3.10.1 source before building. This patch does not contain third-party vendored dependencies.

Before distributing a modified executable, include its matching source and upstream license, record the build revision/toolchain and update these verification results. Keep the original WM available for rollback.

Packaging: the modified executable and watcher are installed in `%USERPROFILE%/.glzr/glazewm/wm`. Both community Start-menu and optional login shortcuts run this executable. The original MSI remains unchanged for CLI compatibility and rollback. Avoid enabling a separate upstream autostart entry. Matching modified source, upstream source, vendored dependencies and licenses are included in the bundle.
