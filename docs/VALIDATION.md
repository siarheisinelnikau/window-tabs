# Validation record

## Session boundaries

Implementation on 2026-10-31 used code-only verification. No application GUI was started, no actual desktop windows were enumerated or manipulated by the application, no screenshots were taken, no update check/download/installer was run, and no remote Git operation or publication was performed. Cargo builds used at most two jobs with a repository-local target directory. Test adapters and filesystem fixtures are synthetic; fixtures stay under `target/test-fixtures` and do not use the real LocalAppData directory.

## Automated checks

- `cargo test --offline --locked --lib --tests`: 33 tests passed (2 controller, 14 group-state, 9 persistence/PE/release-selection, 8 replacement-transaction tests). Coverage includes preserving minimized/unrelated groups after exit, restoration-only retry behavior, journal-write retry, backward-compatible journals, recovery access with corrupt settings, persistent request IDs across coalesced events, strict executable rejection, unchanged-current-EXE checks, existing-sidecar preservation, successful replacement, launch-failure rollback/restart, and explicit rollback/restart failures. File/process effects of replacement tests use an in-memory adapter; the native installer is never executed.
- `cargo clippy --offline --locked --all-targets -- -D warnings`: passed. The environment may emit an incremental-cache hard-link warning and copy files instead; no source warnings remained.
- `cargo fmt --all --check`: passed through `scripts/verify.ps1 -Offline`.
- `scripts/package.ps1 -Offline`: built the optimized x64 MSVC executable and wrote the versioned asset, SHA-256 manifest, license, and third-party notices. Static PE inspection confirmed x64/PE32+, Windows GUI subsystem, embedded PerMonitorV2/asInvoker manifest, and no external MSVC CRT DLL imports. This is not a GUI smoke test.

The reviewed local package built with Rust 1.96.1 is `dist/v0.1.0/WindowTabs-v0.1.0.exe` (923,648 bytes). Its SHA-256 is `d161b176d2054c1dfad825ceee3b17f90708591cc0912aa399b6e3982f2a17a1`. The checksum identifies this local build; it is not a published or signed release.

## Required manual pass before release

Use only disposable application documents and settings for these checks. The items below are **pending**, not reported successes.

| Area | Procedure and expected result |
| --- | --- |
| Basic grouping | Group two ordinary resizable unelevated windows. Only the active member is visible, native tab labels are correct, and switch/previous/next work. |
| Group editing | Add a third window; move it left/right, rename, save, detach, and close. Detached/closed members return to original placement and show state. |
| Multiple groups | Keep two groups and unrelated applications open. Strips follow their members in Z-order, never cover an unrelated foreground application, and never steal focus during background events. |
| Native keyboard/UI | Tab navigation, arrow keys in tab control, Ctrl+Tab with strip/manager focus, list selection, long labels, accessibility names, high contrast, and minimum-size layout work. Shortcuts do not intercept another application's input. |
| Process lifecycle | Close inactive and active target windows, including all windows from one process. The next live tab appears, empty groups vanish, and no recycled HWND receives an old placement. |
| Minimize/maximize | Start with normal, minimized, and maximized members. Group them, minimize/restore via the manager, maximize inside the group, detach, and close. Verify original show states. The restore-to-maximized preference of a window already minimized before capture is not guaranteed by the documented placement-flags output and remains a compatibility check. |
| DPI/monitors | Test 100/150/200% monitors, negative coordinates, top/left taskbar, work-area edges, monitor removal, and display scaling changes. Strip text and geometry use the current monitor DPI; recovery remains reachable. |
| Application constraints | Test typical editors, Explorer, browsers and custom-chrome apps independently. Confirm rejects for elevated, tool, topmost, modal, cloaked, fixed-size, UWP frame, and hung windows. Resolve any application-specific hide/resize or 300 ms timeout behavior. |
| Dialogs | Open a modal dialog on a grouped member, try switching/closing, and then close the dialog and retry. Failed restoration remains recoverable and the parent is not lost. |
| Restoration isolation | Cause a restoration timeout, then move/close members of another group. The recovery-required group remains isolated, with no strip or automatic re-hide/reposition. Retry restoration and the final journal write. |
| Interrupted session | Terminate WindowTabs while a disposable group is active; restart and explicitly recover. Original windows return and exited identities are discarded. Closing before recovery preserves the journal. |
| Persistence | Restart with saved definitions and startup checks disabled. Settings remain, no startup request occurs, manual checking still works, and changed/ambiguous titles fail conservatively. |
| Update errors | With disposable copies, test unpublished/404, offline, HTTP 403/429, wrong assets, wrong checksum, truncated download, read-only installation folder, and helper timeout. Installed EXE and data remain intact. |
| Update installation | Against an authorized published test release, click Download and install. Observe progress, verify group restoration before exit, replacement, one restarted process, installed version, retained settings, backup, and rollback on launch failure. |
| Update retry/races | Refuse installation with pending recovery, then resolve and retry while events arrive. The old refusal must not cancel the retry. Change the installed file before replacement or precreate sidecars; preserve those files. Confirm actual backup/restart error messages against disposable copies. |
| Lifecycle/performance | Close the manager with groups active; verify actual placements and process exit. Launch twice; no duplicate controller appears. Measure idle visible/minimized CPU, event bursts and responsiveness in the packaged release. |
| Screenshots | Capture real manager, tab strip with grouped windows, and Settings / Updates. Add working images and English alt text to README. |

The canonical GitHub repository/release is planned and has not been created by this session. Publication, actual network error paths, and end-to-end updater execution remain unverified.
