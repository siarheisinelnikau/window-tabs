//! The only module that manipulates other applications' windows.
//! No injection, reparenting, style changes, input attachment, or global keyboard hooks.
use crate::{model::*, storage::Store};
use std::{
    ffi::c_void,
    mem::{size_of, zeroed},
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{Dwm::*, Gdi::*},
    Security::*,
    System::Threading::*,
    UI::{HiDpi::*, Input::KeyboardAndMouse::IsWindowEnabled, WindowsAndMessaging::*},
};

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
pub fn hwnd(identity: &Identity) -> HWND {
    identity.handle as usize as HWND
}
fn error(action: &str) -> String {
    format!("{action}: {}", std::io::Error::last_os_error())
}

struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

fn process(pid: u32) -> Result<(String, u64, bool)> {
    unsafe {
        let handle = Handle(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid));
        if handle.0.is_null() {
            return Err(error("Cannot inspect process"));
        }
        let (mut created, mut exited, mut kernel, mut user) =
            (zeroed(), zeroed(), zeroed(), zeroed());
        if GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) == 0 {
            return Err(error("Cannot read process identity"));
        }
        let started = (created.dwHighDateTime as u64) << 32 | created.dwLowDateTime as u64;
        let mut path = vec![0u16; 32768];
        let mut len = path.len() as u32;
        if QueryFullProcessImageNameW(handle.0, 0, path.as_mut_ptr(), &mut len) == 0 {
            return Err(error("Cannot read application path"));
        }
        let mut token = null_mut();
        if OpenProcessToken(handle.0, TOKEN_QUERY, &mut token) == 0 {
            return Err(error("Cannot inspect elevation"));
        }
        let token = Handle(token);
        let mut elevation: TOKEN_ELEVATION = zeroed();
        let mut returned = 0;
        if GetTokenInformation(
            token.0,
            TokenElevation,
            &mut elevation as *mut _ as *mut c_void,
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        ) == 0
        {
            return Err(error("Cannot inspect elevation"));
        }
        Ok((
            String::from_utf16_lossy(&path[..len as usize]),
            started,
            elevation.TokenIsElevated != 0,
        ))
    }
}

fn class(window: HWND) -> String {
    let mut text = [0u16; 256];
    let len = unsafe { GetClassNameW(window, text.as_mut_ptr(), text.len() as i32) };
    String::from_utf16_lossy(&text[..len.max(0) as usize])
}
pub(crate) fn title(window: HWND) -> String {
    let mut text = [0u16; 1024];
    let len = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
    String::from_utf16_lossy(&text[..len.max(0) as usize])
}
pub(crate) fn rect(window: HWND) -> Option<Rect> {
    let mut rect = RECT::default();
    (unsafe { GetWindowRect(window, &mut rect) } != 0).then_some(from_rect(rect))
}
pub fn from_rect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}
fn to_rect(r: Rect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn monitor_geometry(monitor: HMONITOR) -> Option<(Rect, [i32; 2])> {
    let mut info: MONITORINFO = unsafe { zeroed() };
    info.cbSize = size_of::<MONITORINFO>() as u32;
    if unsafe { GetMonitorInfoW(monitor, &mut info) } == 0 {
        return None;
    }
    Some((
        from_rect(info.rcWork),
        [
            info.rcWork.left - info.rcMonitor.left,
            info.rcWork.top - info.rcMonitor.top,
        ],
    ))
}

pub(crate) fn strip_height(window: HWND) -> i32 {
    let mut dpi = 96;
    let mut y = 96;
    unsafe {
        GetDpiForMonitor(
            MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST),
            MDT_EFFECTIVE_DPI,
            &mut dpi,
            &mut y,
        );
    }
    (42 * dpi / 96) as i32
}
pub(crate) fn work_area(window: HWND) -> Rect {
    unsafe {
        let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFO = zeroed();
        info.cbSize = size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(monitor, &mut info) != 0 {
            return from_rect(info.rcWork);
        }
        Rect {
            left: 0,
            top: 0,
            right: 1280,
            bottom: 720,
        }
    }
}

fn placement(window: HWND) -> Result<WINDOWPLACEMENT> {
    let mut placement: WINDOWPLACEMENT = unsafe { zeroed() };
    placement.length = size_of::<WINDOWPLACEMENT>() as u32;
    if unsafe { GetWindowPlacement(window, &mut placement) } == 0 {
        Err(error("Cannot read window placement"))
    } else {
        Ok(placement)
    }
}

fn eligible(window: HWND) -> Result<WindowInfo> {
    unsafe {
        if IsWindow(window) == 0 || IsWindowVisible(window) == 0 || IsWindowEnabled(window) == 0 {
            return Err("Window is hidden, disabled, or no longer exists.".into());
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid == 0 || pid == GetCurrentProcessId() || !GetWindow(window, GW_OWNER).is_null() {
            return Err("Owned windows and WindowTabs windows cannot participate.".into());
        }
        let style = GetWindowLongPtrW(window, GWL_STYLE) as u32;
        let extended = GetWindowLongPtrW(window, GWL_EXSTYLE) as u32;
        if style & WS_CHILD != 0
            || style & WS_CAPTION != WS_CAPTION
            || style & WS_THICKFRAME == 0
            || extended & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_TOPMOST) != 0
        {
            return Err("This special or fixed-size window cannot safely participate.".into());
        }
        let mut cloaked: u32 = 0;
        if DwmGetWindowAttribute(
            window,
            DWMWA_CLOAKED as u32,
            &mut cloaked as *mut _ as *mut c_void,
            4,
        ) < 0
            || cloaked != 0
        {
            return Err("Cloaked windows or another virtual desktop are unsupported.".into());
        }
        if IsHungAppWindow(window) != 0 {
            return Err("The application is not responding.".into());
        }
        let class = class(window);
        if [
            "Progman",
            "WorkerW",
            "Shell_TrayWnd",
            "Shell_SecondaryTrayWnd",
            "ApplicationFrameWindow",
            "ConsoleWindowClass",
        ]
        .contains(&class.as_str())
        {
            return Err("Shell, legacy console, and UWP frame windows are unsupported.".into());
        }
        let title = title(window);
        if title.trim().is_empty() {
            return Err("Untitled windows are unsupported.".into());
        }
        let (executable, started, elevated) = process(pid)?;
        if elevated {
            return Err("Elevated application windows are intentionally unsupported.".into());
        }
        let placement = placement(window)?;
        let bounds = if IsIconic(window) != 0 {
            // WINDOWPLACEMENT uses workspace coordinates for ordinary top-level windows.
            let monitor = MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST);
            let mut info: MONITORINFO = zeroed();
            info.cbSize = size_of::<MONITORINFO>() as u32;
            let mut bounds = from_rect(placement.rcNormalPosition);
            if GetMonitorInfoW(monitor, &mut info) != 0 {
                let dx = info.rcWork.left - info.rcMonitor.left;
                let dy = info.rcWork.top - info.rcMonitor.top;
                bounds.left += dx;
                bounds.right += dx;
                bounds.top += dy;
                bounds.bottom += dy;
            }
            bounds
        } else {
            rect(window).ok_or("Cannot read window bounds.")?
        };
        Ok(WindowInfo {
            identity: Identity {
                handle: window as usize as u64,
                pid,
                started,
                class,
            },
            title,
            executable,
            bounds,
        })
    }
}

pub fn discover() -> Vec<WindowInfo> {
    unsafe extern "system" fn callback(window: HWND, param: LPARAM) -> i32 {
        // SAFETY: EnumWindows invokes this synchronously with the original vector pointer.
        let windows = unsafe { &mut *(param as *mut Vec<WindowInfo>) };
        if windows.len() < 512
            && let Ok(info) = eligible(window)
        {
            windows.push(info);
        }
        1
    }
    let mut windows: Vec<WindowInfo> = vec![];
    unsafe {
        EnumWindows(Some(callback), &mut windows as *mut _ as LPARAM);
    }
    windows.sort_by_key(|w| w.title.to_lowercase());
    windows
}

fn identity_matches(identity: &Identity) -> Result<bool> {
    unsafe {
        let window = hwnd(identity);
        if IsWindow(window) == 0 {
            return Ok(false);
        }
        let current_class = class(window);
        if current_class.is_empty() {
            return Err("Cannot verify the window class. Its recovery entry was retained.".into());
        }
        if current_class != identity.class {
            return Ok(false);
        }
        let mut pid = 0;
        GetWindowThreadProcessId(window, &mut pid);
        if pid != identity.pid {
            return Ok(false);
        }
        process(pid).map(|(_, started, _)| started == identity.started)
    }
}

fn verified(identity: &Identity) -> Result<()> {
    if identity_matches(identity)? {
        Ok(())
    } else {
        Err("The window closed or its handle was reused. Refresh the list.".into())
    }
}

/// Bounded acknowledgment of asynchronous window requests, only during a user action.
/// A timed-out restoration remains in the durable recovery journal.
fn await_state(identity: &Identity, mut check: impl FnMut(HWND) -> bool) -> Result<()> {
    for _ in 0..30 {
        verified(identity)?;
        if check(hwnd(identity)) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    Err("The application did not apply the window change within 300 ms. Close modal dialogs and retry; the recovery journal is retained.".into())
}

fn responsive(window: HWND) -> Result<()> {
    if unsafe { IsHungAppWindow(window) } != 0 {
        return Err("The application is not responding.".into());
    }
    if unsafe { IsWindowEnabled(window) } == 0 {
        return Err(
            "Close the application's modal dialog before switching or detaching its window.".into(),
        );
    }
    Ok(())
}

pub(crate) fn shown(window: HWND) -> bool {
    unsafe {
        let mut cloaked: u32 = 0;
        IsWindowVisible(window) != 0
            && IsIconic(window) == 0
            && GetWindowLongPtrW(window, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0
            && DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED as u32,
                &mut cloaked as *mut _ as *mut c_void,
                4,
            ) >= 0
            && cloaked == 0
    }
}

pub struct NativeAdapter {
    pub store: Store,
}
impl WindowAdapter for NativeAdapter {
    fn capture(&mut self, identity: &Identity) -> Result<Original> {
        verified(identity)?;
        let window = hwnd(identity);
        let info = eligible(window)?;
        let mut response = 0;
        if unsafe {
            SendMessageTimeoutW(
                window,
                WM_NULL,
                0,
                0,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                150,
                &mut response,
            )
        } == 0
        {
            return Err("The application did not respond within 150 ms.".into());
        }
        let p = placement(window)?;
        let (_, offset) =
            monitor_geometry(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) })
                .ok_or("Cannot read the original monitor geometry.")?;
        Ok(Original {
            info,
            placement: Placement {
                flags: p.flags,
                show: p.showCmd,
                min: [p.ptMinPosition.x, p.ptMinPosition.y],
                max: [p.ptMaxPosition.x, p.ptMaxPosition.y],
                normal: from_rect(p.rcNormalPosition),
            },
            visible: unsafe { IsWindowVisible(window) } != 0,
            normal_screen: Some(from_rect(p.rcNormalPosition).translated(offset[0], offset[1])),
        })
    }
    fn alive(&self, identity: &Identity) -> bool {
        // An inspection failure is not evidence of exit. Retain the recovery entry.
        identity_matches(identity).unwrap_or(true)
    }
    fn present(&mut self, identity: &Identity, bounds: Rect) -> Result<()> {
        verified(identity)?;
        let window = hwnd(identity);
        responsive(window)?;
        let mut cloaked: u32 = 0;
        if unsafe {
            DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED as u32,
                &mut cloaked as *mut _ as *mut c_void,
                4,
            )
        } < 0
            || cloaked != 0
            || unsafe { GetWindowLongPtrW(window, GWL_EXSTYLE) } as u32 & WS_EX_TOPMOST != 0
        {
            return Err("The grouped window is now cloaked or topmost. Return it to the current desktop and normal mode before switching.".into());
        }
        unsafe {
            // Posting across input queues keeps the worker independent of the target's event loop.
            if IsIconic(window) != 0 || IsZoomed(window) != 0 {
                ShowWindowAsync(window, SW_RESTORE);
            }
            if SetWindowPos(
                window,
                null_mut(),
                bounds.left,
                bounds.top,
                bounds.width(),
                bounds.height(),
                SWP_NOZORDER | SWP_NOACTIVATE | SWP_ASYNCWINDOWPOS | SWP_SHOWWINDOW,
            ) == 0
            {
                return Err(error("Cannot position application"));
            }
        }
        await_state(identity, |window| unsafe {
            IsWindowVisible(window) != 0
                && IsIconic(window) == 0
                && IsZoomed(window) == 0
                && rect(window) == Some(bounds)
        })
    }
    fn hide(&mut self, identity: &Identity) -> Result<()> {
        verified(identity)?;
        responsive(hwnd(identity))?;
        if unsafe { ShowWindowAsync(hwnd(identity), SW_HIDE) } == 0 {
            Err(error("Cannot hide application"))
        } else {
            await_state(identity, |window| unsafe { IsWindowVisible(window) == 0 })
        }
    }
    fn restore(&mut self, original: &Original) -> Result<()> {
        if !identity_matches(&original.info.identity)? {
            return Ok(());
        }
        let window = hwnd(&original.info.identity);
        responsive(window)?;
        let p = &original.placement;
        let (_, current_offset) =
            monitor_geometry(unsafe { MonitorFromWindow(window, MONITOR_DEFAULTTONEAREST) })
                .ok_or("Cannot read the recovery monitor geometry.")?;
        let screen_normal = original
            .normal_screen
            .unwrap_or_else(|| p.normal.translated(current_offset[0], current_offset[1]));
        let screen_rect = to_rect(screen_normal);
        let offscreen = unsafe { MonitorFromRect(&screen_rect, MONITOR_DEFAULTTONULL).is_null() };
        let normal = if offscreen {
            let (work, offset) = monitor_geometry(unsafe {
                MonitorFromRect(&screen_rect, MONITOR_DEFAULTTONEAREST)
            })
            .ok_or("Cannot choose a recovery monitor.")?;
            screen_normal
                .fit(work, 0)
                .translated(-offset[0], -offset[1])
        } else {
            p.normal
        };
        let restored_placement = WINDOWPLACEMENT {
            length: size_of::<WINDOWPLACEMENT>() as u32,
            // WPF_ASYNCWINDOWPLACEMENT avoids blocking behind a hung application's thread.
            flags: p.flags | WPF_ASYNCWINDOWPLACEMENT,
            showCmd: p.show,
            ptMinPosition: POINT {
                x: p.min[0],
                y: p.min[1],
            },
            ptMaxPosition: POINT {
                x: p.max[0],
                y: p.max[1],
            },
            rcNormalPosition: to_rect(normal),
        };
        unsafe {
            if SetWindowPlacement(window, &restored_placement) == 0 {
                return Err(error("Cannot restore original placement"));
            }
            if !original.visible && ShowWindowAsync(window, SW_HIDE) == 0 {
                return Err(error("Cannot restore original visibility"));
            }
        }
        await_state(&original.info.identity, |window| {
            let Ok(current) = placement(window) else {
                return false;
            };
            (unsafe { IsWindowVisible(window) } != 0) == original.visible
                && current.showCmd == p.show
                && from_rect(current.rcNormalPosition) == normal
        })
    }
    fn focus(&mut self, identity: &Identity) {
        if identity_matches(identity).unwrap_or(false) {
            unsafe {
                SetForegroundWindow(hwnd(identity));
            }
        }
    }
    fn checkpoint(&mut self, originals: &[Original]) -> Result<()> {
        self.store.save_recovery(originals)
    }
}

pub struct Instance(HANDLE);
impl Instance {
    pub fn acquire() -> Result<Self> {
        unsafe {
            let name = wide("Local\\WindowTabs.SingleInstance.v1");
            let handle = CreateMutexW(null(), 0, name.as_ptr());
            if handle.is_null() {
                return Err(error("Cannot create instance lock"));
            }
            if GetLastError() == ERROR_ALREADY_EXISTS {
                CloseHandle(handle);
                return Err(
                    "WindowTabs is already running. Open its manager from the taskbar.".into(),
                );
            }
            Ok(Self(handle))
        }
    }
}
impl Drop for Instance {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}
