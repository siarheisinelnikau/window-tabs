# WindowTabs

Repository: `window-tabs`
Priority: 5 of 15 (user-requested launch order)
Product: WindowTabs
Purpose: Tab groups for related application windows
Inspiration: https://www.stardock.com/products/groupy/

Independent open-source implementation; no affiliation, competitor code or copied assets.

## Requested implementation

Create a Windows utility that groups existing ordinary application windows behind a native tab strip. Users should be able to select windows, name/save groups, switch tabs, reorder/detach members and restore original bounds/visibility when a group closes. Handle target process exit, minimized/maximized states, DPI, multiple monitors and application windows that cannot safely participate.

The deliverable should include real Windows window-discovery/group-management source, not only a launcher or static mockup. Prefer documented window-management techniques and a separate tab-strip window over injection. Do not inject DLLs, patch Explorer, install hooks globally during development, or manipulate the live desktop. Test state transitions with synthetic window adapters; document any unsupported elevated or special application windows for later manual verification.

## Packaging and publication target

Windows standalone asset: `WindowTabs-vVERSION.exe` (for the initial release, `WindowTabs-v0.1.0.exe`). Append architecture only when distributing multiple executable architectures in the same release.
Planned canonical repository: https://github.com/siarheisinelnikau/window-tabs (not yet created or published; keep availability/error reporting truthful).

## Working agreement

Follow AGENTS.md. Complete as much real implementation and safe noninteractive verification as possible in this unattended code-only session. Manual UI, device and hardware tests will happen later. Record required external components and their verification status. Keep changes committed locally in meaningful increments.

