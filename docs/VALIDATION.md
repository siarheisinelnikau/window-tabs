# Validation record

## Session boundaries

Initial implementation and recovery/updater review on 2026-10-31 used code-only verification. During the later interface review, the user explicitly authorized only a hidden, isolated render-save-exit mode. No ordinary GUI session, computer use, actual desktop-window discovery/manipulation, update check/download/installer, or remote Git operation occurred. Cargo builds used at most two jobs with a repository-local target directory. Fixtures and capture data are synthetic; neither tests nor self-capture use real LocalAppData settings.

## Automated checks

- `cargo test --offline --locked --lib --tests`: 35 tests passed (2 controller, 2 presentation, 14 group-state, 9 persistence/PE/release-selection, 8 replacement-transaction tests). Presentation coverage checks minimum/larger layout bounds, pairwise overlap, and unchanged native-list content caching. Existing recovery, journal, update selection, strict executable validation, replacement and rollback regression coverage remains passing. Installer file/process effects use an in-memory adapter; the native installer is never executed.
- `cargo clippy --offline --locked --all-targets -- -D warnings`: passed. The environment may emit an incremental-cache hard-link warning and copy files instead; no source warnings remained.
- `cargo fmt --all --check`: passed through `scripts/verify.ps1 -Offline`.
- `scripts/package.ps1 -Offline`: built the optimized x64 MSVC executable and wrote the versioned asset, SHA-256 manifest, license, and third-party notices. Static PE inspection confirmed x64/PE32+, Windows GUI subsystem, embedded PerMonitorV2/asInvoker manifest, and no external MSVC CRT DLL imports. This is not a GUI smoke test.

The reviewed local package built with Rust 1.96.1 is `dist/v0.1.0/WindowTabs-v0.1.0.exe` (1,134,080 bytes). Its SHA-256 is `af4ef55e13713324d3618f0ec8581a9132093330f0191eaf5f3f190166e9cc6b`. The checksum identifies this local build; it is not a published or signed release.

## Static visual review and load-gate scope

After source inspection of the native GDI path, blocking event loop, and capture isolation, nine final captures were produced directly by the packaged EXE: six main states at 900×640, Groups/Settings at 820×600, and an error state at 900×640, all at 96 DPI. Each owned process exited normally with code 0 after one composite raster pass. Recorded render-save times were 22–31 ms and total whole-process CPU times 31.3–46.9 ms (startup included, timer-resolution limited). These are finite-capture observations, not idle utilization or frame-latency measurements.

The actual PNGs were opened and inspected. Shared layout, typography, selection, disabled buttons, action hierarchy, footer wrapping, and minimum-size clipping were reviewed. Metadata and executable/image hashes are under `docs/screenshots`, including `capture-summary.json` in each capture directory. The pictures show real native client controls with synthetic data, not unrelated application windows, native menus, or the OS title bar.

The normal runtime rendering/load gate remains incomplete: visible/hidden idle CPU, raster/present counts, GPU/waits, keyboard interaction, DPI transitions, and restore cycles were not measured. Source-level hidden-view deferral and content/geometry caches were added; no performance improvement is claimed without those measurements. Details and reproduction commands are in [DESIGN.md](DESIGN.md).

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
| Screenshots | Manager, Settings, picker, empty/saved/recovery/error and minimum-size native client self-captures are complete and linked in README. A screenshot of the tab strip with real grouped desktop windows remains pending. |

The canonical GitHub repository/release is planned and has not been created by this session. Publication, actual network error paths, and end-to-end updater execution remain unverified.
