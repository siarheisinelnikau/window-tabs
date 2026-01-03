# Architecture

## Ownership and flow

`model.rs` implements groups over a `WindowAdapter` interface. It captures every original before the first side effect, checkpoints the combined recovery journal, and only then presents/hides members. Detach/close restore before dropping records. Failed restorations stay in the active group and durable journal. An exited/replaced window is removed without applying its placement to a new process. One-member groups remain useful after a member exits; the final exit removes the group.

`platform.rs` is the Win32 adapter. `EnumWindows` filters ordinary, visible, enabled, captioned, resizable, non-owned, non-topmost windows. Process query access, elevation, DWM cloaking, responsiveness, PID, creation time, and class are checked. It snapshots `WINDOWPLACEMENT` unchanged in workspace coordinates, while group rectangles use physical screen coordinates. Original minimized and maximized states are recovered with `SetWindowPlacement`; workspace coordinates are not passed directly to `SetWindowPos`.

The UI thread owns all manager/strip controls and process-scoped `SetWinEventHook` subscriptions with `WINEVENT_OUTOFCONTEXT`. It runs a blocking `GetMessageW` loop. WinEvent callbacks only post a coalesced notification; a one-shot 75 ms timer bounds bursts. No timer runs while idle and there are no keyboard hooks. The strip is adjacent to the active member in Z-order and is not globally topmost. Target parent/owner/style and application content are not changed.

`controller.rs` owns state on one worker with a 32-command bounded channel. Window enumeration, identity queries, grouping, restoration, settings serialization, and journal writes happen there. Small snapshots wake the native UI through `PostMessageW`; UI reads do not wait for disk/network operations. Window changes use asynchronous native requests and bounded 300 ms acknowledgments on the worker. A timeout is reported and retains restoration records. A 150 ms responsiveness probe is used before capturing a selected window. The UI does not claim a queued OS request is a completed restoration.

`storage.rs` writes versioned JSON to a same-directory temporary file, flushes it, and replaces the destination with `MoveFileExW`. Corrupt/future-schema files block initialization instead of being overwritten. The singleton mutex prevents multiple normal instances from racing over data. Recovery from a previous session is an explicit user action, never an automatic desktop change on startup.

`update.rs` uses public HTTPS through Windows WinHTTP, stable semver comparison, canonical asset paths, a size cap, SHA-256, and an x64 PE check. The UI permits at most one update worker. Progress is coalesced in a one-slot mailbox; downloads do not create unbounded tasks. The same EXE copied into a private application update subdirectory acts as a temporary replacement helper after parent exit. It stages beside the target on the same volume, keeps a backup, replaces atomically, and attempts restart with rollback if process creation fails. This does not detect a new version that launches and then crashes; the backup supports manual recovery. No updater path is exercised by the tests.

The manifest requests `asInvoker` and `PerMonitorV2`. The x64 MSVC configuration links the CRT statically for a single executable. Native standard controls/GDI provide the UI; the application has no graphics render loop, GPU renderer, telemetry, plugin injection, WebView, driver, or service. Idle CPU and perceptual responsiveness have not been measured.

## API references consulted

- [SetWindowPos](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowpos): asynchronous placement, visibility, and Z-order flags.
- [WINDOWPLACEMENT](https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-windowplacement): saved show state, asynchronous restoration, and workspace-coordinate semantics.
- [SetWindowPlacement](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwindowplacement): restoration and OS adjustment of offscreen placements.
- [ShowWindowAsync](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-showwindowasync): request acceptance is distinct from target completion.
- [SetWinEventHook](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setwineventhook): process filtering, out-of-context callbacks, and message-loop requirements.
- [WinHttpSetOption](https://learn.microsoft.com/en-us/windows/win32/api/winhttp/nf-winhttp-winhttpsetoption): request redirect policy.

