# Release notes

## 0.1.0 — unpublished initial preview

WindowTabs introduces native tab groups for related, already-open Windows desktop application windows. This is the first local implementation; there is no preceding published release.

### Added

- A native window picker and group manager with naming, member switching, adding, reordering, detaching, and original-placement restoration.
- Separate native tab strips, a context menu for group actions, process-exit handling, minimized-group restoration, and monitor work-area/DPI calculations.
- Ordered saved group definitions with conservative matching against application paths, classes, and exact titles.
- A durable recovery journal written before window changes, explicit recovery after interrupted sessions, and retention of failed restorations.
- Settings with installed version, optional nonblocking startup update checks, manual update checks, and a user-initiated verified download/install/restart implementation.
- Offline-capable synthetic tests, Windows build checks, and a standalone packaging script.

### Package and known limitations

The intended first asset is **WindowTabs-v0.1.0.exe** for Windows x64, accompanied by **SHA256SUMS.txt**. It is an unelevated portable application; settings live in LocalAppData. No release has been published.

Compilation and synthetic tests do not verify cross-process desktop behavior. Interactive window grouping, DPI/monitor transitions, focus/Z-order, recovery on a real desktop, and live update installation require the manual checks in `docs/VALIDATION.md`. Elevated, special, modal, hung, and cloaked windows are excluded. Native manager/settings client captures are available; the tab strip with actual grouped applications remains uncaptured.

### Fixed during local review

- Inactive-window exit/detachment and exits in another group no longer reveal minimized groups. Failed restorations enter a restoration-only phase so late events cannot regroup windows being restored.
- Failed exit-journal writes remain retryable, and normal shutdown retries the final journal write. A corrupt settings file no longer blocks access to a valid recovery journal or gets overwritten during recovery.
- Offscreen restoration now chooses and acknowledges a reachable placement using physical monitor coordinates; previous recovery journals remain compatible.
- Installation requires successful restoration and uses durable request IDs so coalesced UI events cannot lose installation refusals or reuse stale replies.
- Portable updates verify the current EXE against the captured old hash, create unique files without overwriting existing sidecars, and validate complete executable headers/sections. Failed new-version launches restore and attempt to restart the previous version, with explicit backup paths on rollback failure.
- Recovery/updater review expanded coverage to 33 tests before the interface changes below. Window operations and installer/process side effects remain simulated; live desktop and updater validation are pending.

### Interface improvements

- Split the manager into Groups, Saved groups, and Settings, with a focused New group/Add windows picker, selection prerequisites, useful empty/recovery states, and native primary actions.
- Applied shared typography, spacing and a light slate palette while retaining native keyboard/accessibility controls and high-contrast system colors. Kept restore-and-exit on close; no unverified hide-to-tray lifecycle was introduced.
- Avoided rebuilding unchanged lists, updating hidden/minimized manager views, or relaying out hidden/unchanged tab strips. Actual idle/hidden performance remains unmeasured.
- Added an isolated developer self-capture mode that renders native client controls directly to PNG and exits without visible windows, user data, window discovery or network requests. Nine final images were statically reviewed, including Settings and minimum-size/error states.
- Added two presentation regressions for 35 passing tests, and updated release packaging/notices for PNG encoding. Interactive window grouping, input, focus, DPI transitions and live updater validation remain pending.
- Added original labeled action/navigation icons, colored window-type badges, two-line list rows, a stacked-tab application logo, and a soft violet palette with rounded controls and padded fields. The standalone EXE includes a multi-resolution icon. Static captures exposed and resolved truncated Rename, Detach and Delete saved labels.
- Kept off-page lists hidden during retained-content refresh instead of reenabling their visibility with redraw. The isolated capture now checks this invariant after a data refresh. Two more presentation regressions cover display-title preservation and icon-category fallback, bringing the total to 37 passing tests.

