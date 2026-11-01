# Validation record

## 0.2.0 localization and theme verification — 2026-11-01

- `cargo test --offline --locked --all-targets`: 33 tests passed. Coverage includes all 148 catalog messages and placeholder counts, parameterized update/status text, preservation of titles and saved group names, Russian count forms, legacy-settings migration with saved templates/update opt-out, atomic preference round trips, and the existing group/recovery/updater regressions.
- The native font-width regression uses a memory GDI DC and Segoe UI at the production 15-pixel size. Every translated action caption fits its icon/padding allowance at the minimum 820×600 layout. The structural layout regression also passes all page/population/recovery combinations. These checks do not establish complete visual layout or DPI behavior.
- English/Russian/French/German/Spanish/Italian cover navigation, manager/picker/settings controls, group metadata, group and tab-strip menus, recovery instructions, application eligibility messages, and update status/progress. Source-English worker protocol strings remain stable; the presentation layer translates only known UI messages, preserving parameter data and unknown OS/network diagnostic details. Native OS menu styling follows Windows.
- Source inspection: the existing blocking `GetMessageW` loop, bounded worker and event coalescing remain. Preference saves occur in the worker on an explicit action. A changed preference retranslates retained controls and invalidates visible surfaces once; hidden manager/strips retain state, with paint guards and no new recurring timer.
- The repository’s code-only/no-launch restriction remains in force. No ordinary GUI launch, self-capture, real application grouping, update request/install, OS input, or user settings/device change occurred. Refreshed main/Settings screenshots, actual native dark tab-strip appearance, focus/input, DPI, and measured idle/hidden frame/CPU/present behavior remain unverified. Existing images below belong to 0.1.0.

The remaining record describes historical **0.1.0** verification and its manual desktop matrix.

## Session boundaries

Initial implementation and recovery/updater review on 2026-10-31 used code-only verification. During the later interface review, the user explicitly authorized only a hidden, isolated render-save-exit mode. No ordinary GUI session, computer use, actual desktop-window discovery/manipulation, update check/download/installer, or remote Git operation occurred. Cargo builds used at most two jobs with a repository-local target directory. Fixtures and capture data are synthetic; neither tests nor self-capture use real LocalAppData settings.

## Automated checks

- `cargo test --offline --locked --lib --tests`: 37 tests passed (2 controller, 4 presentation, 14 group-state, 9 persistence/PE/release-selection, 8 replacement-transaction tests). Presentation coverage checks minimum/larger layout bounds and pairwise overlap with/without recovery, unchanged native-list content caching, display sanitization without changing matching titles, and application icon categories/fallback. Existing recovery, journal, update selection, strict executable validation, replacement and rollback regression coverage remains passing. Installer file/process effects use an in-memory adapter; the native installer is never executed.
- `cargo clippy --offline --locked --all-targets -- -D warnings`: passed. The environment may emit an incremental-cache hard-link warning and copy files instead; no source warnings remained.
- `cargo fmt --all --check`: passed through `scripts/verify.ps1 -Offline`.
- `scripts/package.ps1 -Offline`: built the optimized x64 MSVC executable and wrote the versioned asset, SHA-256 manifest, license, and third-party notices. Static PE inspection confirmed x64/PE32+, Windows GUI subsystem, embedded PerMonitorV2/asInvoker manifest, icon image/icon-group resources, and no external MSVC CRT DLL imports. The logo ICO was regenerated locally with `scripts/generate-icon.ps1`. This is not a GUI smoke test.

The reviewed local package built with Rust 1.96.1 is `dist/v0.1.0/WindowTabs-v0.1.0.exe` (1,169,920 bytes). Its SHA-256 is `3be106bb090e9e952e3404467976ec5b3cd1c49f70fa18e3a1c212a82c1ef224`. The checksum identifies this local build; it is not a published or signed release.

## Static visual review and load-gate scope

After source inspection of the native GDI path, blocking event loop, and capture isolation, nine final captures were produced directly by the packaged EXE: six main states at 900×640, Groups/Settings at 820×600, and an error state at 900×640, all at 96 DPI. Each owned process exited normally with code 0 after one composite raster pass. Recorded render-save times were 23–35 ms and total whole-process CPU times 31.3–78.1 ms (startup included, timer-resolution limited). Each invocation refreshed retained list content with the layout cache settled, then verified that all off-page controls remained hidden. These are finite-capture observations, not idle utilization or frame-latency measurements.

The actual final PNGs were opened and inspected. Original icons, two-line rows, padded fields, shared layout, typography, selection, disabled buttons, action hierarchy, footer wrapping, and minimum-size clipping were reviewed. Clipped Rename, Detach and Delete saved labels found in intermediate images were corrected before final capture. Metadata and executable/image hashes are under `docs/screenshots`, including `capture-summary.json` in each capture directory. The pictures show real native client controls with synthetic data, not unrelated application windows, native menus, or the OS title bar.

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


## Shared updater, icons and hidden paints — 2026-11-01

This pass used code-only compilation/fixture tests and resource-only inspection. The application was not launched, no real desktop windows were enumerated/manipulated, and no update request/download/installer or computer-use tool ran. Earlier screenshots and capture timings above remain evidence for their recorded build; they do not certify the new binary's idle/hidden behavior.

- Replaced the duplicated release/download/verification/install implementation with a thin `rust-auto-updator` adapter. The product still requires x64 GUI PE images, `WindowTabs-vVERSION.exe`, `SHA256SUMS.txt`, explicit download/install, saved startup opt-out and completed group restoration. Shared `Prepared` staging is dropped on refusal; a helper-launch error now keeps the manager open and recreates its completed window worker so retry remains possible. Old released hosts use a copy of their own old EXE as installer, so their previous transaction does not depend on new helper CLI parsing.
- `cargo fmt --all --check`, `cargo clippy --offline --locked --all-targets -- -D warnings`, and `cargo test --offline --locked --lib --tests` passed (28 tests: 6 unit, 14 grouping/recovery, 8 persistence/shared-release/PE). Generic installation transaction and network verification are now owned/tested by the shared core; product selection, GUI PE enforcement, corrupt settings, restoration and request identity regressions remain here.
- Optimized x64 MSVC compilation and `scripts/package.ps1 -Offline` passed. The final local asset is `dist/v0.1.0/WindowTabs-v0.1.0.exe` (1,869,824 bytes), SHA-256 `cd84ee49d63502beefc7650f210502c5700fa10537dbd1a52b0fa21832e0fb70`. PE inspection confirms GUI subsystem, PerMonitorV2/asInvoker manifest, icon resources, native GDI imports and no external MSVC CRT imports.
- Resource rendering found a concrete identity defect that static resource-presence checks missed: the icon group name embedded by the RC file included literal quote characters, while runtime loading requested the unquoted name. Changed both packaging and native loaders to numeric resource ID 1. Added native 20px/40px variants to the original artwork and load explicit small/large Windows metrics for window/taskbar identity, refreshing those handles on DPI changes. WindowTabs intentionally has no tray mode or tray icon.
- `scripts/inspect-icons.ps1` loaded the final EXE as data and checked each native 16/20/24/32/40/48/64/128/256 HICON for expected size and nonempty alpha. Every icon was drawn through `DrawIconEx` on light/dark backgrounds; the actual [PNG](validation/icons-2026-11-01/icons.png) was opened and inspected. [JSON](validation/icons-2026-11-01/icons.json) records native alpha bounds and executable hash. Packaging now runs this native resource proof, catching resource-name/loading failures as well as missing/blank icons. This is not a screenshot of Explorer/taskbar/OS chrome.
- Runtime source remains CPU native controls/GDI and a blocking `GetMessageW` event loop, with no permanent frame timer. Added visibility/minimization guards to manager `WM_PAINT`, erase, print-client and owner-draw raster paths; they validate paint requests without rasterizing a hidden manager. The explicitly finite capture-only path remains permitted. List/text/strip caches and one-shot WinEvent debounce remain event-driven. Shared updater calls do not introduce a repaint timer.

Visible idle CPU, idle/hidden actual raster/present counters, GPU/presentation waits, interaction/restore, and live window/taskbar integration remain unmeasured for this binary under the repository's code-only restriction. The rendering/load gate is incomplete, not passed from a build or icon PNG. Readiness for interactive desktop use is not claimed.

The declared Rust minimum is now 1.95 because the shared updater requires it. Keep `../rust-auto-updator/crates/rust-auto-updator` beside the checkout; standalone CI must provision that local sibling. The workflow reports the missing prerequisite without inventing a remote checkout. Updated third-party notices include the shared MIT license and only the resolved Windows x64 build graph. No publication occurred.
