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

Compilation and synthetic tests do not verify cross-process desktop behavior. Interactive window grouping, DPI/monitor transitions, focus/Z-order, recovery on a real desktop, and live update installation require the manual checks in `docs/VALIDATION.md`. Elevated, special, modal, hung, and cloaked windows are excluded. Real manager and settings screenshots remain required before publication.

