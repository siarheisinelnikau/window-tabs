# Interface and rendering decisions

## Normal journey

Choose **New group**, select two or more open windows, name the workspace, and create it. The manager returns to **Groups** with that group selected. Switching happens from the native tab strip or the member list. Adding more windows reuses the picker. Saving, adding, and ungrouping live under **Group actions**; the member's direct switch/reorder/detach actions remain alongside its list. Saved definitions and updates each have a deliberate, separate page.

Empty and interrupted-session screens show one useful next action. They do not display a disabled wall of controls. Selection count and prerequisites explain the picker; Back leaves the applications untouched. Errors retain the picker and selection. Owner-drawn native buttons/lists retain their text, keyboard behavior and focus rectangles. Enter uses the current screen's enabled primary action. Actual keyboard and accessibility traversal remain unverified.

The utility's compact everyday surface is its existing tab strip. The manager can be minimized to the taskbar. A tray/hide-on-close lifecycle was considered but deliberately not added: closing the manager remains the explicit restore-all-and-exit path, and an extra hidden lifecycle would need its own interactive validation. This redesign does not change group restoration semantics.

## Shared visual system

- Lavender background `#F8F7FC`, white edit/list surfaces, primary text `#26243A`, secondary text `#69667C`, borders `#E4E0F0`, violet accent `#6354D3`, selection `#EDE9FD`. Mint, blue, amber and peach badges distinguish window types without competing with the primary action.
- Original GDI line icons accompany button labels, navigation and section headings. A stacked-tab logo identifies the app and its empty/recovery states. Rounded white work areas, padded name fields and restrained tinted selection replace the office-style black borders and solid blue list rows. Desktop Organizer informed the visual direction; no external artwork or icon font is loaded.
- Native `BS_OWNERDRAW` buttons and `LBS_OWNERDRAWFIXED | LBS_HASSTRINGS` lists retain actual captions and platform input. Each row shows a title plus application/state detail. Browser, files, editor, terminal and calendar glyphs use executable names with a generic fallback; badges are category illustrations, not extracted application logos. Display sanitization leaves original titles and saved matching data intact. High contrast uses system colors, including highlight text for selected rows/buttons; runtime verification is pending.
- Segoe UI: 27 px semibold product name, 17 px semibold section titles, 15 px body and 12 px row detail at 96 DPI. Fonts are retained and replaced on DPI changes, not allocated for each row.
- 24 px outer margin, 8/12/16/24 px spacing rhythm, 34–38 px controls and 54 px list rows. Primary actions use violet fill; all actions keep text and keyboard focus cues. Rename, Detach and Delete saved were widened after real captures exposed clipped labels.
- Logical client minimum 820×600, initial window around 940×700 including native chrome. Minimum outer bounds are calculated with the actual frame and DPI; DPI changes invalidate the font/layout/list-extent caches.

Layout coordinates are shared by production and capture in `presentation.rs`. Tests check bounds and pairwise control overlap at minimum/larger sizes for populated/empty screens. Native screenshots were reviewed at 900×640 and 820×600, 96 DPI. Review removed unnecessary horizontal scrollbars, the blank empty-group list, and a divider touching the action row.

The original logo's checked-in SVG and 16/24/32/48/64/128/256 px ICO are built from code-native shapes. `scripts/generate-icon.ps1` regenerates the ICO; `build.rs` embeds it with the installed Windows SDK resource compiler. The production manager/strip window classes request that resource. Static packaging checks require both icon image and icon-group resources; native title-bar/taskbar rendering is outside the client capture.

## Quiet rendering

The renderer remains native Win32 controls plus GDI, with a blocking `GetMessageW` loop. There is no animation/render loop or GPU API backend. A bounded 75 ms WinEvent debounce remains a command-dispatch mechanism, not a repaint clock.

Manager snapshots defer visual work while hidden/minimized. Changed rows update retained native controls; identical content leaves selection, scroll, and the native list alone. Window identity changes invalidate picker bindings even when captions are unchanged. Text and enabled states are written only when different. Tab strips defer layout/text updates while hidden, and retain their geometry/size until it changes. None of these source checks substitutes for measured idle/hidden behavior.

Off-page lists stay hidden during content refresh: redraw suppression is toggled only for lists already carrying `WS_VISIBLE`. This avoids the documented [WM_SETREDRAW visibility side effect](https://learn.microsoft.com/en-us/windows/win32/gdi/wm-setredraw). Icon drawing restores the GDI context and releases temporary pens/brushes; it adds no timer, hover polling, Shell icon extraction or renderer dependency.

## Isolated self-capture

`WindowTabs --capture ABSOLUTE.png --screen SCREEN [--size WIDTHxHEIGHT]` is a developer-only entry point parsed before normal startup. Supported screens are groups, empty, picker, saved, settings, recovery, and error. It rejects unknown arguments, existing outputs, and oversized/undersized viewports. It creates a hidden, unowned client window and the real manager's controls, injects non-sensitive data, and paints them once via `WM_PRINT` into a GDI DIB. That bitmap is encoded as PNG; no pixels are read from the desktop.

The capture constructor uses a disconnected worker and a never-read Store path. Sending commands, checking/downloading updates, creating tab strips for targets, and registering event hooks are guarded off. It never acquires the ordinary instance mutex, opens LocalAppData, enumerates windows, or displays a top-level window. Native controls are printed individually without visibility checks so the hidden parent does not turn the PNG into a blank image. Errors go to redirected stderr with a nonzero exit code instead of a dialog.

The renderer has a ten-second internal deadline and the script a twenty-second external timeout; only its owned disposable process can be terminated. Output is completed from a temporary PNG; per-image metadata and a summary record scope, viewport, backend, executable/image hashes, CPU time, and normal exit. These captures verify the client surface only, including forced fixture selection. Native chrome, menus, taskbar behavior, actual target tab strips, normal activation, and focus are not captured.

Each capture also refreshes retained list content after the layout cache settles and checks that only the requested page's controls have `WS_VISIBLE`. This catches accidental reappearance of off-page lists without interacting with the desktop. Actual child bounds, including the padded edit field, determine capture origins.

The rendering/load gate remains **incomplete for ordinary runtime**. Static captures are the explicitly authorized finite diagnostic exception. No settled visible/minimized idle interval, hidden raster/present counters, GPU/presentation timings, or restore cycle was measured. Continue those checks before extended interaction or a public release; do not infer a performance pass from these PNGs.
