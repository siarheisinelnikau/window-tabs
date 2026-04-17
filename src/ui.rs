//! Native controls and a blocking GetMessage loop. All disk/network work is off-thread.
use crate::{
    controller::{self, Command, GroupView, Snapshot, Worker},
    model::*,
    platform::{self, wide},
    presentation::{self, *},
    storage::Store,
    update::{self, Available},
};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    mem::{size_of, zeroed},
    path::PathBuf,
    ptr::{null, null_mut},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicIsize, Ordering},
    },
    thread,
};
use windows_sys::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::{LibraryLoader::*, SystemServices::SS_OWNERDRAW},
        UI::{
            Accessibility::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::w,
};
#[path = "ui_art.rs"]
mod art;

const MAIN_CLASS: *const u16 = w!("WindowTabs.Manager.v1");
const STRIP_CLASS: *const u16 = w!("WindowTabs.Strip.v1");
const STATE: u32 = WM_APP + 1;
const EVENTS: u32 = WM_APP + 2;
const TAB: u32 = WM_APP + 3;
const STRIP_ACTION: u32 = WM_APP + 4;
const UPDATE: u32 = WM_APP + 5;
const EVENT_TIMER: usize = 1;
const STRIP_TABS: i32 = 200;
const STRIP_MENU: i32 = 201;
const STRIP_CLOSE: i32 = 202;

#[derive(Clone, Copy)]
struct Palette {
    background: COLORREF,
    surface: COLORREF,
    text: COLORREF,
    secondary: COLORREF,
    border: COLORREF,
    accent: COLORREF,
    accent_down: COLORREF,
    on_accent: COLORREF,
    selection: COLORREF,
    disabled: COLORREF,
    high_contrast: bool,
}
fn palette() -> Palette {
    unsafe {
        let mut contrast: HIGHCONTRASTW = zeroed();
        contrast.cbSize = size_of::<HIGHCONTRASTW>() as u32;
        if !CAPTURE_MODE.load(Ordering::Relaxed) {
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                &mut contrast as *mut _ as *mut std::ffi::c_void,
                0,
            );
        }
        if !CAPTURE_MODE.load(Ordering::Relaxed) && contrast.dwFlags & HCF_HIGHCONTRASTON != 0 {
            Palette {
                background: GetSysColor(COLOR_WINDOW),
                surface: GetSysColor(COLOR_WINDOW),
                text: GetSysColor(COLOR_WINDOWTEXT),
                secondary: GetSysColor(COLOR_WINDOWTEXT),
                border: GetSysColor(COLOR_WINDOWTEXT),
                accent: GetSysColor(COLOR_HIGHLIGHT),
                accent_down: GetSysColor(COLOR_HIGHLIGHT),
                on_accent: GetSysColor(COLOR_HIGHLIGHTTEXT),
                selection: GetSysColor(COLOR_HIGHLIGHT),
                disabled: GetSysColor(COLOR_WINDOW),
                high_contrast: true,
            }
        } else {
            Palette {
                background: art::rgb(248, 247, 252),
                surface: 0x00ffffff,
                text: art::rgb(38, 36, 58),
                secondary: art::rgb(105, 102, 124),
                border: art::rgb(228, 224, 240),
                accent: art::rgb(99, 84, 211),
                accent_down: art::rgb(78, 64, 183),
                on_accent: art::rgb(255, 255, 255),
                selection: art::rgb(237, 233, 253),
                disabled: art::rgb(238, 237, 245),
                high_contrast: false,
            }
        }
    }
}
fn paint_background(window: HWND, dc: HDC) {
    unsafe {
        let theme = palette();
        let brush = CreateSolidBrush(theme.background);
        let mut client = RECT::default();
        GetClientRect(window, &mut client);
        FillRect(dc, &client, brush);
        DeleteObject(brush);
        let dpi = GetDpiForWindow(window).max(96) as i32;
        art::brand(dc, 24 * dpi / 96, 24 * dpi / 96, 40 * dpi / 96);
        // Read only our retained child controls; drawing never borrows App state.
        for id in [ID_GROUPS, ID_MEMBERS, ID_WINDOWS, ID_SAVED, ID_NAME] {
            let child = GetDlgItem(window, id);
            if !child.is_null() && GetWindowLongPtrW(child, GWL_STYLE) as u32 & WS_VISIBLE != 0 {
                let mut bounds = RECT::default();
                GetWindowRect(child, &mut bounds);
                MapWindowPoints(
                    null_mut(),
                    window,
                    &mut bounds as *mut RECT as *mut POINT,
                    2,
                );
                if id == ID_NAME {
                    InflateRect(&mut bounds, 10 * dpi / 96, 8 * dpi / 96);
                } else {
                    InflateRect(&mut bounds, 6 * dpi / 96, 6 * dpi / 96);
                }
                art::rounded(dc, bounds, theme.surface, theme.border, 12 * dpi / 96);
            }
        }
        let pen = CreatePen(PS_SOLID, 1, theme.border);
        let old = SelectObject(dc, pen);
        for y in [138 * dpi / 96, client.bottom - 62 * dpi / 96] {
            MoveToEx(dc, 24 * dpi / 96, y, null_mut());
            LineTo(dc, client.right - 24 * dpi / 96, y);
        }
        SelectObject(dc, old);
        DeleteObject(pen);
    }
}

static MANAGER: AtomicIsize = AtomicIsize::new(0);
static EVENT_PENDING: AtomicBool = AtomicBool::new(false);
static CAPTURE_MODE: AtomicBool = AtomicBool::new(false);
thread_local! {
    static APP: RefCell<Option<App>> = const { RefCell::new(None) };
    // Out-of-context WinEvent callbacks run on the UI thread that registered them.
    static EVENT_WINDOWS: RefCell<BTreeSet<isize>> = const { RefCell::new(BTreeSet::new()) };
}

enum UpdateNotice {
    Checked(std::result::Result<Option<Available>, String>),
    Progress(u64, u64),
    Downloaded(std::result::Result<PathBuf, String>),
}
struct Strip {
    window: HWND,
    tabs: HWND,
    signature: String,
    font: HFONT,
    height: i32,
    geometry: Option<(i32, i32, i32, i32)>,
    shown: bool,
}
struct App {
    window: HWND,
    controls: HashMap<i32, HWND>,
    worker: Worker,
    snapshot: Snapshot,
    group_ids: Vec<GroupId>,
    strips: HashMap<GroupId, Strip>,
    hook_pids: BTreeSet<u32>,
    hooks: Vec<HWINEVENTHOOK>,
    font: HFONT,
    startup_processed: bool,
    update_busy: bool,
    available: Option<Available>,
    update_mailbox: Arc<Mutex<Option<UpdateNotice>>>,
    cancel: Arc<AtomicBool>,
    store: Store,
    install_plan: Option<PathBuf>,
    install_request_id: u64,
    closing: bool,
    page: Page,
    capture: bool,
    dirty_manager: bool,
    list_cache: ContentCache,
    displayed_windows: Vec<Identity>,
    member_ids: Vec<Identity>,
    picker_pending: bool,
    saved_pending: bool,
    preferred_group: Option<GroupId>,
    last_status: String,
    pending_text: RefCell<HashMap<i32, String>>,
    title_font: HFONT,
    heading_font: HFONT,
    detail_font: HFONT,
    layout_key: Option<(Page, i32, i32, bool, bool, bool)>,
}

pub fn fatal(message: &str) {
    unsafe {
        MessageBoxW(
            null_mut(),
            wide(message).as_ptr(),
            w!("WindowTabs"),
            MB_OK | MB_ICONERROR,
        );
    }
}

/// Isolated developer mode: native client controls -> one GDI bitmap -> PNG -> exit.
/// No singleton, runtime worker, event subscriptions, network or settings are started.
pub fn capture() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if ![5, 7].contains(&args.len()) || args[3] != "--screen" {
        return Err("Usage: WindowTabs --capture ABSOLUTE.png --screen groups|empty|picker|saved|settings|recovery".into());
    }
    let path = PathBuf::from(&args[2]);
    if !path.is_absolute()
        || path
            .extension()
            .is_none_or(|e| !e.eq_ignore_ascii_case("png"))
        || path.exists()
        || path.with_extension("capture.json").exists()
    {
        return Err("Choose a new absolute PNG output path.".into());
    }
    let screen = args[4].to_str().ok_or("Invalid screen name.")?;
    let (page, snapshot) = crate::capture_fixture::fixture(screen)?;
    let (width, height) = if args.len() == 7 {
        if args[5] != "--size" {
            return Err("Expected --size WIDTHxHEIGHT.".into());
        }
        let text = args[6].to_str().ok_or("Invalid viewport.")?;
        let (w, h) = text.split_once('x').ok_or("Expected WIDTHxHEIGHT.")?;
        let w = w.parse::<i32>().map_err(|e| e.to_string())?;
        let h = h.parse::<i32>().map_err(|e| e.to_string())?;
        if !(MIN_WIDTH..=1600).contains(&w) || !(MIN_HEIGHT..=1000).contains(&h) {
            return Err("Capture viewport must be 820x600 to 1600x1000 logical pixels.".into());
        }
        (w, h)
    } else {
        (900, 640)
    };
    let started = std::time::Instant::now();
    CAPTURE_MODE.store(true, Ordering::Relaxed);
    unsafe {
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_UNAWARE);
        let init = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_TAB_CLASSES,
        };
        if InitCommonControlsEx(&init) == 0 {
            return Err("Cannot initialize native controls.".into());
        }
        let class = WNDCLASSW {
            lpfnWndProc: Some(main_proc),
            hInstance: GetModuleHandleW(null()),
            lpszClassName: MAIN_CLASS,
            hCursor: LoadCursorW(null_mut(), IDC_ARROW),
            ..zeroed()
        };
        if RegisterClassW(&class) == 0 {
            return Err("Cannot register capture window.".into());
        }
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            MAIN_CLASS,
            w!("WindowTabs isolated capture"),
            WS_POPUP | WS_CLIPCHILDREN,
            0,
            0,
            width,
            height,
            null_mut(),
            null_mut(),
            GetModuleHandleW(null()),
            null(),
        );
        if window.is_null() {
            return Err("Cannot create hidden capture controls.".into());
        }
        let mut app = App::new(
            window,
            Store::new(path.parent().unwrap().join("unused-capture-data")),
            true,
        )?;
        app.snapshot = snapshot;
        app.page = page;
        app.render_manager();
        app.layout();
        // Exercise a data refresh with the layout cache already settled. Hidden
        // list controls must remain hidden when their retained content changes.
        for id in [ID_WINDOWS, ID_GROUPS, ID_MEMBERS, ID_SAVED] {
            app.list_cache.invalidate(id);
        }
        app.dirty_manager = true;
        app.render_manager();
        if screen == "picker" || screen == "error" {
            for index in [0, 1] {
                SendMessageW(app.controls[&ID_WINDOWS], LB_SETSEL, 1, index);
            }
            app.availability();
        }
        if screen == "saved" {
            SendMessageW(app.controls[&ID_SAVED], LB_SETCURSEL, 0, 0);
            app.availability();
        }
        app.set(ID_UPDATE_STATUS,if screen=="settings" {"Updates have not been checked. Choose Check for updates to check the stable release channel."} else {""});
        let placements = presentation::layout(
            page,
            width,
            height,
            !app.snapshot.groups.is_empty(),
            !app.snapshot.settings.templates.is_empty(),
            app.snapshot.recovery > 0,
        );
        for (&id, &control) in &app.controls {
            let expected = placements.iter().any(|p| p.id == id);
            let shown = GetWindowLongPtrW(control, GWL_STYLE) as u32 & WS_VISIBLE != 0;
            if shown != expected {
                return Err(format!(
                    "Control {id} changed visibility during a retained-content refresh."
                ));
            }
        }
        APP.with(|state| *state.borrow_mut() = Some(app));
        let bitmap = CaptureBitmap::new(width, height)?;
        paint_background(window, bitmap.dc);
        for p in &placements {
            if started.elapsed() > std::time::Duration::from_secs(10) {
                return Err("Capture exceeded the ten-second rendering deadline.".into());
            }
            let control = APP.with(|state| state.borrow().as_ref().unwrap().controls[&p.id]);
            let mut bounds = RECT::default();
            GetWindowRect(control, &mut bounds);
            MapWindowPoints(
                null_mut(),
                window,
                &mut bounds as *mut RECT as *mut POINT,
                2,
            );
            let saved = SaveDC(bitmap.dc);
            SetViewportOrgEx(bitmap.dc, bounds.left, bounds.top, null_mut());
            IntersectClipRect(
                bitmap.dc,
                0,
                0,
                bounds.right - bounds.left,
                bounds.bottom - bounds.top,
            );
            let mut result = 0;
            let painted = SendMessageTimeoutW(
                control,
                WM_PRINT,
                bitmap.dc as usize,
                (PRF_CLIENT | PRF_NONCLIENT | PRF_ERASEBKGND) as isize,
                SMTO_ABORTIFHUNG | SMTO_BLOCK,
                500,
                &mut result,
            );
            RestoreDC(bitmap.dc, saved);
            if painted == 0 {
                return Err(format!("Native control {} did not complete capture.", p.id));
            }
        }
        GdiFlush();
        let bgra =
            std::slice::from_raw_parts(bitmap.bits as *const u8, (width * height * 4) as usize);
        let mut rgba = vec![0u8; bgra.len()];
        for (from, to) in bgra.chunks_exact(4).zip(rgba.chunks_exact_mut(4)) {
            to.copy_from_slice(&[from[2], from[1], from[0], 255]);
        }
        let temporary = path.with_extension("png.tmp");
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|e| e.to_string())?;
        let mut encoder =
            png::Encoder::new(std::io::BufWriter::new(file), width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer.write_image_data(&rgba).map_err(|e| e.to_string())?;
        writer.finish().map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, &path).map_err(|e| e.to_string())?;
        DestroyWindow(window);
        APP.with(|state| {
            state.borrow_mut().take();
        });
        let metadata = serde_json::json!({"screen":screen,"viewport":[width,height],"dpi":96,"backend":"Win32 native controls / GDI DIB / WM_PRINT","scope":"client area; synthetic non-sensitive data; no visible windows","native_controls":placements.len(),"render_passes":1,"elapsed_ms":started.elapsed().as_millis(),"executable":std::env::current_exe().map_err(|e|e.to_string())?});
        std::fs::write(
            path.with_extension("capture.json"),
            serde_json::to_vec_pretty(&metadata).unwrap(),
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

struct CaptureBitmap {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bits: *mut std::ffi::c_void,
}
impl CaptureBitmap {
    unsafe fn new(width: i32, height: i32) -> Result<Self> {
        unsafe {
            let dc = CreateCompatibleDC(null_mut());
            if dc.is_null() {
                return Err("Cannot create the software capture DC.".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    ..zeroed()
                },
                ..zeroed()
            };
            let mut bits = null_mut();
            let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if bitmap.is_null() || bits.is_null() {
                DeleteDC(dc);
                return Err("Cannot allocate the capture bitmap.".into());
            }
            let previous = SelectObject(dc, bitmap);
            Ok(Self {
                dc,
                bitmap,
                previous,
                bits,
            })
        }
    }
}
impl Drop for CaptureBitmap {
    fn drop(&mut self) {
        unsafe {
            SelectObject(self.dc, self.previous);
            DeleteObject(self.bitmap);
            DeleteDC(self.dc);
        }
    }
}

pub fn run() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    if args.get(1).is_some_and(|arg| arg == "--apply-update") {
        let path = args
            .get(2)
            .ok_or("The updater requires an installation plan.")?;
        let result = update::native::apply(&PathBuf::from(path));
        if let Err(error) = &result
            && let Some(parent) = PathBuf::from(path).parent()
        {
            let _ = std::fs::write(parent.join("install-error.txt"), error);
        }
        return result;
    }
    if args.len() > 1 {
        return Err("Unknown argument. Start WindowTabs without arguments.".into());
    }
    let _instance = platform::Instance::acquire()?;
    let root = std::env::var_os("LOCALAPPDATA").ok_or("LOCALAPPDATA is unavailable.")?;
    let store = Store::new(PathBuf::from(root).join("WindowTabs"));
    unsafe {
        SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_TAB_CLASSES | ICC_STANDARD_CLASSES,
        };
        if InitCommonControlsEx(&controls) == 0 {
            return Err("Cannot initialize native controls.".into());
        }
        let instance = GetModuleHandleW(null());
        for (name, procedure) in [
            (
                MAIN_CLASS,
                Some(main_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
            ),
            (
                STRIP_CLASS,
                Some(strip_proc as unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT),
            ),
        ] {
            let class = WNDCLASSW {
                lpfnWndProc: procedure,
                hInstance: instance,
                lpszClassName: name,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hIcon: LoadIconW(instance, w!("WINDOWTABS_ICON")),
                hbrBackground: (COLOR_BTNFACE + 1) as usize as HBRUSH,
                ..zeroed()
            };
            if RegisterClassW(&class) == 0 {
                return Err(format!(
                    "Cannot register window class: {}",
                    std::io::Error::last_os_error()
                ));
            }
        }
        let dpi = GetDpiForSystem().max(96) as i32;
        let mut work = RECT::default();
        SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut work as *mut _ as *mut std::ffi::c_void,
            0,
        );
        let window = CreateWindowExW(
            0,
            MAIN_CLASS,
            w!("WindowTabs — Window groups"),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            (940 * dpi / 96).min((work.right - work.left).max(MIN_WIDTH)),
            (700 * dpi / 96).min((work.bottom - work.top).max(MIN_HEIGHT)),
            null_mut(),
            null_mut(),
            instance,
            null(),
        );
        if window.is_null() {
            return Err(format!(
                "Cannot create manager: {}",
                std::io::Error::last_os_error()
            ));
        }
        MANAGER.store(window as isize, Ordering::Relaxed);
        let app = App::new(window, store, false)?;
        APP.with(|state| *state.borrow_mut() = Some(app));
        APP.with(|state| {
            if let Some(app) = state.borrow_mut().as_mut() {
                app.layout();
            }
        });
        ShowWindow(window, SW_SHOW);
        let mut message: MSG = zeroed();
        loop {
            let result = GetMessageW(&mut message, null_mut(), 0, 0);
            if result == 0 {
                break;
            }
            if result == -1 {
                return Err(format!(
                    "Message loop failed: {}",
                    std::io::Error::last_os_error()
                ));
            }
            if message.message == WM_KEYDOWN
                && message.wParam == VK_TAB as usize
                && GetKeyState(VK_CONTROL as i32) < 0
            {
                APP.with(|state| {
                    if let Ok(mut state) = state.try_borrow_mut()
                        && let Some(app) = state.as_mut()
                    {
                        let root = GetAncestor(message.hwnd, GA_ROOT);
                        let group = app
                            .strips
                            .iter()
                            .find(|(_, s)| s.window == root)
                            .map(|(id, _)| *id)
                            .or_else(|| app.selected_group());
                        if let Some(id) = group {
                            app.send(Command::Cycle(
                                id,
                                if GetKeyState(VK_SHIFT as i32) < 0 {
                                    -1
                                } else {
                                    1
                                },
                            ));
                        }
                    }
                });
                continue;
            }
            if IsDialogMessageW(window, &message) == 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
        MANAGER.store(0, Ordering::Relaxed);
        APP.with(|state| {
            state.borrow_mut().take();
        });
    }
    Ok(())
}

impl App {
    unsafe fn new(window: HWND, store: Store, capture: bool) -> Result<Self> {
        let worker = if capture {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            drop(receiver);
            Worker {
                sender,
                mailbox: Arc::new(Mutex::new(None)),
            }
        } else {
            controller::start(store.clone(), window as isize, STATE)?
        };
        let mut app = Self {
            window,
            controls: HashMap::new(),
            worker,
            snapshot: Snapshot::default(),
            group_ids: vec![],
            strips: HashMap::new(),
            hook_pids: BTreeSet::new(),
            hooks: vec![],
            font: null_mut(),
            startup_processed: false,
            update_busy: false,
            available: None,
            update_mailbox: Arc::new(Mutex::new(None)),
            cancel: Arc::new(AtomicBool::new(false)),
            store,
            install_plan: None,
            install_request_id: 0,
            closing: false,
            page: Page::Groups,
            capture,
            dirty_manager: true,
            list_cache: ContentCache::default(),
            displayed_windows: vec![],
            member_ids: vec![],
            picker_pending: false,
            saved_pending: false,
            preferred_group: None,
            last_status: String::new(),
            pending_text: RefCell::new(HashMap::new()),
            title_font: null_mut(),
            heading_font: null_mut(),
            detail_font: null_mut(),
            layout_key: None,
        };
        unsafe {
            for (id, text) in [
                (ID_LABEL_WINDOWS, "Choose windows"),
                (ID_LABEL_GROUPS, "Your groups"),
                (ID_LABEL_MEMBERS, "Windows in this group"),
                (ID_LABEL_NAME, "Group name"),
                (ID_LABEL_SAVED, "Saved groups"),
                (ID_LABEL_SETTINGS, "Updates"),
                (ID_BRAND, "WindowTabs"),
                (ID_TAGLINE, "Keep related windows together."),
                (ID_HELP, ""),
                (ID_EMPTY, "No groups yet"),
                (
                    ID_EMPTY_HELP,
                    "Create a group from two or more open windows. Switch between them from a small tab strip above the active window.",
                ),
                (ID_VERSION, ""),
                (ID_PICK_COUNT, "0 windows selected"),
                (ID_EMPTY_ART, "Grouped windows illustration"),
            ] {
                app.control(id, "STATIC", text, 0)?;
            }
            app.control(
                ID_WINDOWS,
                "LISTBOX",
                "",
                WS_BORDER
                    | WS_VSCROLL
                    | WS_HSCROLL
                    | LBS_EXTENDEDSEL as u32
                    | LBS_NOINTEGRALHEIGHT as u32
                    | LBS_NOTIFY as u32
                    | WS_TABSTOP,
            )?;
            for id in [ID_GROUPS, ID_MEMBERS, ID_SAVED] {
                app.control(
                    id,
                    "LISTBOX",
                    "",
                    WS_BORDER
                        | WS_VSCROLL
                        | WS_HSCROLL
                        | LBS_NOINTEGRALHEIGHT as u32
                        | LBS_NOTIFY as u32
                        | WS_TABSTOP,
                )?;
            }
            app.control(
                ID_NAME,
                "EDIT",
                "My workspace",
                WS_BORDER | ES_AUTOHSCROLL as u32 | WS_TABSTOP,
            )?;
            SendMessageW(app.controls[&ID_NAME], EM_SETLIMITTEXT, 80, 0);
            for (id, text) in [
                (ID_NAV_GROUPS, "Groups"),
                (ID_NAV_SAVED, "Saved groups"),
                (ID_NAV_SETTINGS, "Settings"),
                (ID_NEW, "New group"),
                (ID_CANCEL, "Back"),
                (ID_MORE, "Group actions…"),
                (ID_REFRESH, "Refresh windows"),
                (ID_CREATE, "Create group"),
                (ID_ADD, "Add to group"),
                (ID_RENAME, "Rename"),
                (ID_SWITCH, "Switch"),
                (ID_LEFT, "Left"),
                (ID_RIGHT, "Right"),
                (ID_DETACH, "Detach"),
                (ID_CLOSE, "Close group"),
                (ID_SAVE, "Save group"),
                (ID_RESTORE, "Restore saved"),
                (ID_DELETE, "Delete saved"),
                (ID_RECOVER, "Recover windows"),
                (ID_CHECK, "Check for updates"),
                (ID_INSTALL, "Download and install"),
            ] {
                app.control(
                    id,
                    "BUTTON",
                    text,
                    WS_TABSTOP
                        | if [ID_CREATE, ID_SWITCH, ID_RESTORE].contains(&id) {
                            BS_DEFPUSHBUTTON
                        } else {
                            BS_PUSHBUTTON
                        } as u32,
                )?;
            }
            app.control(
                ID_STARTUP,
                "BUTTON",
                "Check for updates on startup",
                WS_TABSTOP | BS_AUTOCHECKBOX as u32,
            )?;
            app.control(ID_STATUS, "STATIC", "Loading eligible windows…", 0)?;
            app.control(
                ID_UPDATE_STATUS,
                "STATIC",
                &format!(
                    "Installed version: {}. Updates have not been checked.",
                    crate::VERSION
                ),
                0,
            )?;
            app.update_font();
            EnableWindow(app.controls[&ID_INSTALL], 0);
        }
        Ok(app)
    }
    unsafe fn control(&mut self, id: i32, class: &str, text: &str, style: u32) -> Result<()> {
        let mut style = style;
        if class == "BUTTON" && style & BS_TYPEMASK as u32 != BS_AUTOCHECKBOX as u32 {
            style = (style & !(BS_TYPEMASK as u32)) | BS_OWNERDRAW as u32;
        }
        if class == "LISTBOX" {
            style = (style & !(WS_HSCROLL | WS_BORDER))
                | LBS_OWNERDRAWFIXED as u32
                | LBS_HASSTRINGS as u32;
        }
        if class == "EDIT" {
            style &= !WS_BORDER;
        }
        if class == "STATIC"
            && [
                ID_LABEL_GROUPS,
                ID_LABEL_MEMBERS,
                ID_LABEL_WINDOWS,
                ID_LABEL_SAVED,
                ID_LABEL_SETTINGS,
                ID_VERSION,
                ID_EMPTY_ART,
            ]
            .contains(&id)
        {
            style |= SS_OWNERDRAW;
        }
        let control = unsafe {
            CreateWindowExW(
                0,
                wide(class).as_ptr(),
                wide(text).as_ptr(),
                WS_CHILD | WS_VISIBLE | style,
                0,
                0,
                10,
                10,
                self.window,
                id as usize as HMENU,
                GetModuleHandleW(null()),
                null(),
            )
        };
        if control.is_null() {
            return Err(format!("Cannot create control {id}."));
        }
        self.controls.insert(id, control);
        Ok(())
    }
    fn dpi(&self) -> i32 {
        if self.capture {
            96
        } else {
            unsafe { GetDpiForWindow(self.window) }.max(96) as i32
        }
    }
    fn scale(&self, value: i32) -> i32 {
        value * self.dpi() / 96
    }
    fn layout(&mut self) {
        if !self.capture && unsafe { IsIconic(self.window) } != 0 {
            return;
        }
        let mut client = RECT::default();
        unsafe {
            GetClientRect(self.window, &mut client);
        }
        let width = client.right * 96 / self.dpi();
        let height = client.bottom * 96 / self.dpi();
        let key = (
            self.page,
            width,
            height,
            !self.snapshot.groups.is_empty(),
            !self.snapshot.settings.templates.is_empty(),
            self.snapshot.recovery > 0,
        );
        if self.layout_key == Some(key) {
            return;
        }
        self.layout_key = Some(key);
        let placements = presentation::layout(self.page, width, height, key.3, key.4, key.5);
        for (&id, &control) in &self.controls {
            if let Some(p) = placements.iter().find(|p| p.id == id) {
                // The native edit sits inside a padded, parent-painted field.
                let (x, y, width, height) = if id == ID_NAME {
                    (p.x + 10, p.y + 8, p.width - 20, p.height - 16)
                } else {
                    (p.x, p.y, p.width, p.height)
                };
                unsafe {
                    SetWindowPos(
                        control,
                        null_mut(),
                        self.scale(x),
                        self.scale(y),
                        self.scale(width),
                        self.scale(height),
                        SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                    );
                }
            } else {
                unsafe {
                    ShowWindow(control, SW_HIDE);
                }
            }
        }
        self.set(ID_HELP,match self.page {
            Page::Picker(Some(_))=>"Select windows to add to this group. Use Ctrl or Shift to select several.",
            Page::Picker(None)=>"Select at least two open windows. Use Ctrl or Shift to select several.",
            Page::Saved=>"Restore a saved group using windows that are already open with the same titles.",
            Page::Settings=>"Keep WindowTabs up to date. Downloads and installation start only when you choose them.",
            _=>"",
        });
        if self.page == Page::Saved {
            self.set(ID_EMPTY, "No saved groups yet");
            self.set(ID_EMPTY_HELP,"Open a group's actions and choose Save group. Its window order and titles will be ready to restore next time.");
        } else {
            self.set(ID_EMPTY, "A little less window switching");
            self.set(ID_EMPTY_HELP,"Create a group from two or more open windows. Your windows stay independent, with a small tab strip to switch between them.");
        }
        if self.snapshot.recovery > 0 && self.page == Page::Groups {
            self.set(ID_EMPTY, "Let's recover your windows");
            self.set(ID_EMPTY_HELP,"WindowTabs was interrupted while windows were grouped. Recover their original positions before starting a new workspace.");
        }
        self.set(
            ID_CREATE,
            if matches!(self.page, Page::Picker(Some(_))) {
                "Add windows"
            } else {
                "Create group"
            },
        );
        self.set(ID_VERSION, &format!("WindowTabs {}", crate::VERSION));
        for (id, page) in [
            (ID_NAV_GROUPS, Page::Groups),
            (ID_NAV_SAVED, Page::Saved),
            (ID_NAV_SETTINGS, Page::Settings),
        ] {
            unsafe {
                let active = isize::from(self.page == page);
                if GetWindowLongPtrW(self.controls[&id], GWLP_USERDATA) != active {
                    SetWindowLongPtrW(self.controls[&id], GWLP_USERDATA, active);
                    InvalidateRect(self.controls[&id], null(), 0);
                }
            }
        }
        unsafe {
            InvalidateRect(self.window, null(), 1);
        }
    }
    fn navigate(&mut self, page: Page) {
        if page == Page::Groups
            && matches!(self.page, Page::Picker(_))
            && let Some(group) = self
                .selected_group()
                .and_then(|id| self.snapshot.groups.iter().find(|g| g.id == id))
        {
            self.set(ID_NAME, &group.name);
        }
        self.page = page;
        self.layout_key = None;
        self.dirty_manager = true;
        self.render_manager();
        self.layout();
        self.availability();
        if self.snapshot.recovery == 0 {
            self.set(ID_STATUS,match page {Page::Settings=>"Startup checks are optional. Manual checking stays available.",Page::Saved=>"Saved groups match open windows; they do not launch applications.",Page::Picker(_)=>"Ungrouping keeps your applications open and restores their original positions.",_=>"Select a group to manage its windows, or choose New group."});
        }
        let focus = match page {
            Page::Picker(_) => ID_WINDOWS,
            Page::Saved => ID_SAVED,
            Page::Settings => ID_STARTUP,
            Page::Groups => ID_GROUPS,
        };
        if !self.capture {
            unsafe {
                SetFocus(self.controls[&focus]);
            }
        }
    }
    fn update_font(&mut self) {
        unsafe {
            let new = CreateFontW(
                -self.scale(15),
                0,
                0,
                0,
                FW_NORMAL as i32,
                0,
                0,
                0,
                DEFAULT_CHARSET as u32,
                OUT_DEFAULT_PRECIS as u32,
                CLIP_DEFAULT_PRECIS as u32,
                CLEARTYPE_QUALITY as u32,
                DEFAULT_PITCH as u32,
                w!("Segoe UI"),
            );
            if !new.is_null() {
                for control in self.controls.values() {
                    SendMessageW(*control, WM_SETFONT, new as usize, 1);
                }
                if !self.font.is_null() {
                    DeleteObject(self.font);
                }
                self.font = new;
                let detail = CreateFontW(
                    -self.scale(12),
                    0,
                    0,
                    0,
                    FW_NORMAL as i32,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    0,
                    0,
                    CLEARTYPE_QUALITY as u32,
                    0,
                    w!("Segoe UI"),
                );
                if !detail.is_null() {
                    art::set_detail_font(detail);
                    if !self.detail_font.is_null() {
                        DeleteObject(self.detail_font);
                    }
                    self.detail_font = detail;
                }
                for id in [ID_GROUPS, ID_MEMBERS, ID_WINDOWS, ID_SAVED] {
                    SendMessageW(
                        self.controls[&id],
                        LB_SETITEMHEIGHT,
                        0,
                        self.scale(54) as isize,
                    );
                }
                for font in [&mut self.title_font, &mut self.heading_font] {
                    if !font.is_null() {
                        DeleteObject(*font);
                    }
                }
                self.title_font = CreateFontW(
                    -self.scale(27),
                    0,
                    0,
                    0,
                    600,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    0,
                    0,
                    CLEARTYPE_QUALITY as u32,
                    0,
                    w!("Segoe UI"),
                );
                self.heading_font = CreateFontW(
                    -self.scale(17),
                    0,
                    0,
                    0,
                    600,
                    0,
                    0,
                    0,
                    DEFAULT_CHARSET as u32,
                    0,
                    0,
                    CLEARTYPE_QUALITY as u32,
                    0,
                    w!("Segoe UI"),
                );
                SendMessageW(
                    self.controls[&ID_BRAND],
                    WM_SETFONT,
                    self.title_font as usize,
                    1,
                );
                for id in [
                    ID_LABEL_WINDOWS,
                    ID_LABEL_GROUPS,
                    ID_LABEL_MEMBERS,
                    ID_LABEL_SAVED,
                    ID_LABEL_SETTINGS,
                    ID_EMPTY,
                    ID_VERSION,
                ] {
                    SendMessageW(
                        self.controls[&id],
                        WM_SETFONT,
                        self.heading_font as usize,
                        1,
                    );
                }
            }
        }
    }
    fn set(&self, id: i32, text: &str) {
        if !self.capture
            && unsafe { IsWindowVisible(self.window) == 0 || IsIconic(self.window) != 0 }
        {
            self.pending_text.borrow_mut().insert(id, text.to_owned());
            return;
        }
        unsafe {
            let mut current =
                vec![0u16; (GetWindowTextLengthW(self.controls[&id]) + 1).max(1) as usize];
            let length = GetWindowTextW(
                self.controls[&id],
                current.as_mut_ptr(),
                current.len() as i32,
            );
            if String::from_utf16_lossy(&current[..length.max(0) as usize]) != text {
                SetWindowTextW(self.controls[&id], wide(text).as_ptr());
            }
        }
    }
    fn fill(&mut self, id: i32, rows: Vec<String>) -> bool {
        if self.list_cache.changed(id, rows.clone()) {
            list_fill(self.controls[&id], rows.into_iter());
            true
        } else {
            false
        }
    }
    fn send(&mut self, command: Command) {
        if self.capture {
            return;
        }
        let install = matches!(&command, Command::InstallAndQuit(_));
        if let Err(error) = self.worker.sender.try_send(command) {
            self.set(
                ID_STATUS,
                &format!("Window worker is busy or unavailable: {error}. Try again."),
            );
            self.closing = false;
            if install {
                self.install_plan = None;
                self.set(ID_UPDATE_STATUS, "Update was not installed because the window worker could not accept the request. Retry Download and install.");
                unsafe {
                    EnableWindow(
                        self.controls[&ID_INSTALL],
                        i32::from(self.available.is_some() && !self.update_busy),
                    );
                }
            }
        }
    }
    fn selected_group(&self) -> Option<GroupId> {
        selected(self.controls[&ID_GROUPS]).and_then(|i| self.group_ids.get(i).copied())
    }
    fn selected_windows(&self) -> Vec<Identity> {
        unsafe {
            let list = self.controls[&ID_WINDOWS];
            let count = SendMessageW(list, LB_GETSELCOUNT, 0, 0);
            if count <= 0 {
                return vec![];
            }
            let mut indices = vec![0i32; count as usize];
            SendMessageW(
                list,
                LB_GETSELITEMS,
                count as usize,
                indices.as_mut_ptr() as isize,
            );
            indices
                .into_iter()
                .filter_map(|i| self.displayed_windows.get(i as usize).cloned())
                .collect()
        }
    }
    fn name(&self) -> String {
        let mut text = [0u16; 256];
        let count = unsafe {
            GetWindowTextW(
                self.controls[&ID_NAME],
                text.as_mut_ptr(),
                text.len() as i32,
            )
        };
        String::from_utf16_lossy(&text[..count.max(0) as usize])
    }
    fn member(&self) -> Option<usize> {
        selected(self.controls[&ID_MEMBERS])
    }
    fn select_group(&mut self) {
        if let Some(id) = self.selected_group()
            && let Some(group) = self.snapshot.groups.iter().find(|g| g.id == id)
        {
            self.set(ID_NAME, &group.name);
            self.members(&group.clone(), None);
        }
    }
    fn members(&mut self, group: &GroupView, prefer: Option<Identity>) {
        let list = self.controls[&ID_MEMBERS];
        self.fill(
            ID_MEMBERS,
            group
                .members
                .iter()
                .enumerate()
                .map(|(i, w)| window_row(w, i == group.active))
                .collect(),
        );
        self.member_ids = group.members.iter().map(|w| w.identity.clone()).collect();
        let index = prefer
            .and_then(|id| group.members.iter().position(|w| w.identity == id))
            .unwrap_or(group.active);
        unsafe {
            if selected(list) != Some(index) {
                SendMessageW(list, LB_SETCURSEL, index, 0);
            }
        }
    }
    fn command(&mut self, id: i32, notification: u16) {
        if id == ID_GROUPS && notification == LBN_SELCHANGE as u16 {
            self.select_group();
            self.availability();
            return;
        }
        if (id == ID_WINDOWS || id == ID_MEMBERS || id == ID_SAVED)
            && notification == LBN_SELCHANGE as u16
        {
            self.availability();
            return;
        }
        if id == ID_MEMBERS && notification != LBN_DBLCLK as u16 {
            return;
        }
        if notification != BN_CLICKED as u16 && id != ID_MEMBERS {
            return;
        }
        let group = self.selected_group();
        let member = self.member();
        match id {
            ID_NAV_GROUPS => self.navigate(Page::Groups),
            ID_NAV_SAVED => self.navigate(Page::Saved),
            ID_NAV_SETTINGS => self.navigate(Page::Settings),
            ID_CANCEL => self.navigate(Page::Groups),
            ID_NEW => {
                self.set(ID_NAME, "My workspace");
                self.navigate(Page::Picker(None));
                self.send(Command::Refresh);
            }
            ID_MORE => self.group_menu(),
            ID_REFRESH => {
                self.hook_pids.clear();
                self.send(Command::Refresh);
            }
            ID_CREATE => {
                self.picker_pending = true;
                match self.page {
                    Page::Picker(Some(id)) => self.send(Command::Add(id, self.selected_windows())),
                    _ => self.send(Command::Create(self.name(), self.selected_windows())),
                }
                self.availability();
            }
            ID_RECOVER => self.send(Command::Recover),
            ID_STARTUP => self.send(Command::Startup(
                unsafe { SendMessageW(self.controls[&ID_STARTUP], BM_GETCHECK, 0, 0) }
                    == BST_CHECKED as isize,
            )),
            ID_CHECK => self.check_update(),
            ID_INSTALL => self.download_update(),
            ID_RESTORE | ID_DELETE => {
                if let Some(index) = selected(self.controls[&ID_SAVED]) {
                    self.saved_pending = id == ID_RESTORE;
                    self.send(if id == ID_RESTORE {
                        Command::Restore(index)
                    } else {
                        Command::Delete(index)
                    });
                } else {
                    self.set(ID_STATUS, "Select a saved group first.");
                }
            }
            _ => {
                if let Some(group) = group {
                    match id {
                        ID_ADD => {
                            self.navigate(Page::Picker(Some(group)));
                            self.send(Command::Refresh);
                        }
                        ID_RENAME => self.send(Command::Rename(group, self.name())),
                        ID_SAVE => self.send(Command::Save(group)),
                        ID_CLOSE => self.send(Command::Close(group)),
                        ID_SWITCH | ID_MEMBERS => {
                            if let Some(member) = member {
                                self.send(Command::Activate(group, member));
                            }
                        }
                        ID_DETACH => {
                            if let Some(member) = member {
                                self.send(Command::Detach(group, member));
                            }
                        }
                        ID_LEFT | ID_RIGHT => {
                            if let Some(member) = member {
                                let to = if id == ID_LEFT {
                                    member.saturating_sub(1)
                                } else {
                                    member + 1
                                };
                                self.send(Command::Reorder(group, member, to));
                            }
                        }
                        _ => {}
                    }
                } else {
                    self.set(ID_STATUS, "Select an active group first.");
                }
            }
        }
    }
    fn state(&mut self) {
        let snapshot = self
            .worker
            .mailbox
            .lock()
            .ok()
            .and_then(|mut output| output.take());
        let Some(snapshot) = snapshot else {
            return;
        };
        self.snapshot = snapshot;
        self.dirty_manager = true;
        if self.picker_pending {
            if self.snapshot.status.starts_with("Group created.")
                || self.snapshot.status.starts_with("Windows added")
            {
                self.preferred_group = match self.page {
                    Page::Picker(Some(id)) => Some(id),
                    _ => self.snapshot.groups.iter().map(|g| g.id).max(),
                };
                self.picker_pending = false;
                self.page = Page::Groups;
                self.layout_key = None;
            } else if self.snapshot.status.starts_with("Error:") {
                self.picker_pending = false;
            }
        }
        if self.saved_pending && self.snapshot.status.starts_with("Saved group restored") {
            self.saved_pending = false;
            self.page = Page::Groups;
            self.layout_key = None;
            self.preferred_group = self.snapshot.groups.iter().map(|g| g.id).max();
        } else if self.saved_pending && self.snapshot.status.starts_with("Error:") {
            self.saved_pending = false;
        }
        self.render_manager();
        if !self.snapshot.quit {
            self.closing = false;
        }
        if self.install_plan.is_some()
            && self.snapshot.install_result == Some((self.install_request_id, false))
        {
            self.install_plan = None;
            self.set(ID_UPDATE_STATUS, &format!("Update was not installed: {}. Resolve recovery errors and click Download and install again.", self.snapshot.status));
            unsafe {
                EnableWindow(
                    self.controls[&ID_INSTALL],
                    i32::from(self.available.is_some() && !self.update_busy),
                );
            }
        }
        self.refresh_strips();
        self.refresh_hooks();
        if self.snapshot.ready && !self.startup_processed {
            self.startup_processed = true;
            if self.snapshot.settings.check_on_startup {
                self.check_update();
            }
        }
        if self.snapshot.quit {
            if self.snapshot.install_result == Some((self.install_request_id, true))
                && let Some(plan) = self.install_plan.take()
                && let Err(error) = update::native::launch_helper(&plan)
            {
                fatal(&error);
            }
            unsafe {
                DestroyWindow(self.window);
            }
        }
    }
    fn render_manager(&mut self) {
        if !self.capture
            && unsafe { IsWindowVisible(self.window) == 0 || IsIconic(self.window) != 0 }
        {
            return;
        }
        // Network progress may change while the manager is minimized. Publish only the latest text on restore.
        let pending = std::mem::take(&mut *self.pending_text.borrow_mut());
        for (id, text) in pending {
            self.set(id, &text);
        }
        if !self.dirty_manager {
            return;
        }
        self.dirty_manager = false;
        let previous_group = self.selected_group();
        let chosen_group = self.preferred_group.take().or(previous_group);
        let chosen_member = self.member().and_then(|i| self.member_ids.get(i)).cloned();
        let chosen_windows = self.selected_windows();
        let saved = selected(self.controls[&ID_SAVED]);
        let rows = self
            .snapshot
            .windows
            .iter()
            .map(|w| window_row(w, false))
            .collect();
        let new_ids: Vec<_> = self
            .snapshot
            .windows
            .iter()
            .map(|w| w.identity.clone())
            .collect();
        if new_ids != self.displayed_windows {
            self.list_cache.invalidate(ID_WINDOWS);
        }
        if self.fill(ID_WINDOWS, rows) {
            self.displayed_windows = self
                .snapshot
                .windows
                .iter()
                .map(|w| w.identity.clone())
                .collect();
            for (i, id) in self.displayed_windows.iter().enumerate() {
                if chosen_windows.contains(id) {
                    unsafe {
                        SendMessageW(self.controls[&ID_WINDOWS], LB_SETSEL, 1, i as isize);
                    }
                }
            }
        }
        self.fill(
            ID_GROUPS,
            self.snapshot
                .groups
                .iter()
                .map(|g| {
                    group_row(
                        &g.name,
                        g.members.len(),
                        if g.restoring {
                            "Needs recovery"
                        } else if !g.shown {
                            "Minimized"
                        } else {
                            ""
                        },
                    )
                })
                .collect(),
        );
        self.group_ids = self.snapshot.groups.iter().map(|g| g.id).collect();
        if !self.group_ids.is_empty() {
            let index = chosen_group
                .and_then(|id| self.group_ids.iter().position(|g| *g == id))
                .unwrap_or(0);
            unsafe {
                if selected(self.controls[&ID_GROUPS]) != Some(index) {
                    SendMessageW(self.controls[&ID_GROUPS], LB_SETCURSEL, index, 0);
                }
            }
            let group = self.snapshot.groups[index].clone();
            self.members(&group, chosen_member);
            if !matches!(self.page, Page::Picker(_)) && previous_group != Some(group.id) {
                self.set(ID_NAME, &group.name);
            }
        } else {
            self.fill(ID_MEMBERS, vec![]);
            self.member_ids.clear();
        }
        self.fill(
            ID_SAVED,
            self.snapshot
                .settings
                .templates
                .iter()
                .map(|t| group_row(&t.name, t.members.len(), "Saved"))
                .collect(),
        );
        if let Some(index) = saved
            && index < self.snapshot.settings.templates.len()
        {
            unsafe {
                if selected(self.controls[&ID_SAVED]) != Some(index) {
                    SendMessageW(self.controls[&ID_SAVED], LB_SETCURSEL, index, 0);
                }
            }
        }
        unsafe {
            let checked = if self.snapshot.settings.check_on_startup {
                BST_CHECKED
            } else {
                BST_UNCHECKED
            } as usize;
            if SendMessageW(self.controls[&ID_STARTUP], BM_GETCHECK, 0, 0) != checked as isize {
                SendMessageW(self.controls[&ID_STARTUP], BM_SETCHECK, checked, 0);
            }
        }
        if self.snapshot.recovery > 0 {
            self.set(ID_STATUS,"Some windows need recovery from your previous session. Choose Recover windows before creating groups.");
        } else if self.last_status != self.snapshot.status {
            self.set(ID_STATUS, &self.snapshot.status);
            self.last_status = self.snapshot.status.clone();
        }
        self.layout();
        self.availability();
    }
    fn availability(&self) {
        let group = self
            .selected_group()
            .and_then(|id| self.snapshot.groups.iter().find(|g| g.id == id));
        let member = self.member();
        let count = self.selected_windows().len();
        self.set(ID_PICK_COUNT, &format!("{count} windows selected"));
        let ready = self.snapshot.ready && self.snapshot.recovery == 0;
        let active = group.is_some_and(|g| !g.restoring);
        unsafe {
            for (id, enabled) in [
                (ID_NEW, ready),
                (
                    ID_CREATE,
                    ready
                        && !self.picker_pending
                        && count
                            >= if matches!(self.page, Page::Picker(Some(_))) {
                                1
                            } else {
                                2
                            },
                ),
                (ID_MORE, group.is_some()),
                (ID_RENAME, active),
                (ID_SWITCH, active && member.is_some()),
                (ID_DETACH, group.is_some() && member.is_some()),
                (ID_LEFT, active && member.is_some_and(|i| i > 0)),
                (
                    ID_RIGHT,
                    active && member.is_some_and(|i| i + 1 < group.unwrap().members.len()),
                ),
                (
                    ID_RESTORE,
                    ready && selected(self.controls[&ID_SAVED]).is_some(),
                ),
                (ID_DELETE, selected(self.controls[&ID_SAVED]).is_some()),
            ] {
                if (IsWindowEnabled(self.controls[&id]) != 0) != enabled {
                    EnableWindow(self.controls[&id], i32::from(enabled));
                }
            }
        }
    }
    fn group_menu(&mut self) {
        let Some(group) = self
            .selected_group()
            .and_then(|id| self.snapshot.groups.iter().find(|g| g.id == id))
        else {
            return;
        };
        unsafe {
            let menu = CreatePopupMenu();
            for (id, label) in [
                (ID_ADD, "Add windows…"),
                (ID_SAVE, "Save group"),
                (ID_CLOSE, "Ungroup and restore windows"),
            ] {
                AppendMenuW(
                    menu,
                    MF_STRING
                        | if group.restoring && id != ID_CLOSE {
                            MF_GRAYED
                        } else {
                            0
                        },
                    id as usize,
                    wide(label).as_ptr(),
                );
            }
            let mut bounds = RECT::default();
            GetWindowRect(self.controls[&ID_MORE], &mut bounds);
            let action = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                bounds.left,
                bounds.bottom,
                0,
                self.window,
                null(),
            );
            DestroyMenu(menu);
            if action > 0 {
                self.command(action, BN_CLICKED as u16);
            }
        }
    }
    fn refresh_strips(&mut self) {
        if self.capture {
            return;
        }
        let ids: BTreeSet<_> = self.snapshot.groups.iter().map(|g| g.id).collect();
        let removed: Vec<_> = self
            .strips
            .keys()
            .filter(|id| !ids.contains(id))
            .copied()
            .collect();
        for id in removed {
            if let Some(strip) = self.strips.remove(&id) {
                unsafe {
                    DestroyWindow(strip.window);
                    if !strip.font.is_null() {
                        DeleteObject(strip.font);
                    }
                }
            }
        }
        for group in &self.snapshot.groups {
            unsafe {
                if !group.shown && !self.strips.contains_key(&group.id) {
                    continue;
                }
                let strip = self.strips.entry(group.id).or_insert_with(|| {
                    let window = CreateWindowExW(
                        WS_EX_TOOLWINDOW,
                        STRIP_CLASS,
                        wide(&group.name).as_ptr(),
                        WS_POPUP | WS_BORDER | WS_CLIPCHILDREN,
                        0,
                        0,
                        100,
                        42,
                        null_mut(),
                        null_mut(),
                        GetModuleHandleW(null()),
                        null(),
                    );
                    SetWindowLongPtrW(window, GWLP_USERDATA, group.id as isize);
                    let tabs = CreateWindowExW(
                        0,
                        WC_TABCONTROLW,
                        w!(""),
                        WS_CHILD
                            | WS_VISIBLE
                            | WS_TABSTOP
                            | TCS_BUTTONS
                            | TCS_SINGLELINE
                            | TCS_TOOLTIPS,
                        0,
                        0,
                        100,
                        40,
                        window,
                        STRIP_TABS as usize as HMENU,
                        GetModuleHandleW(null()),
                        null(),
                    );
                    for (id, text) in [(STRIP_MENU, "…"), (STRIP_CLOSE, "×")] {
                        CreateWindowExW(
                            0,
                            w!("BUTTON"),
                            wide(text).as_ptr(),
                            WS_CHILD | WS_VISIBLE | WS_TABSTOP,
                            0,
                            0,
                            30,
                            40,
                            window,
                            id as usize as HMENU,
                            GetModuleHandleW(null()),
                            null(),
                        );
                    }
                    Strip {
                        window,
                        tabs,
                        signature: String::new(),
                        font: null_mut(),
                        height: 0,
                        geometry: None,
                        shown: false,
                    }
                });
                if !group.shown {
                    if strip.shown {
                        ShowWindow(strip.window, SW_HIDE);
                        strip.shown = false;
                    }
                    continue;
                }
                let signature = format!(
                    "{}:{:?}",
                    group.name,
                    group
                        .members
                        .iter()
                        .map(|w| (&w.identity, &w.title))
                        .collect::<Vec<_>>()
                );
                if strip.window.is_null()
                    || strip.tabs.is_null()
                    || GetDlgItem(strip.window, STRIP_MENU).is_null()
                    || GetDlgItem(strip.window, STRIP_CLOSE).is_null()
                {
                    SetWindowTextW(
                        self.controls[&ID_STATUS],
                        w!("Cannot create the tab strip. Restoring the affected group."),
                    );
                    let _ = self.worker.sender.try_send(Command::Close(group.id));
                    continue;
                }
                if strip.signature != signature {
                    SendMessageW(strip.tabs, TCM_DELETEALLITEMS, 0, 0);
                    for (i, member) in group.members.iter().enumerate() {
                        let mut label = wide(&member.title);
                        let item = TCITEMW {
                            mask: TCIF_TEXT,
                            pszText: label.as_mut_ptr(),
                            ..zeroed()
                        };
                        SendMessageW(strip.tabs, TCM_INSERTITEMW, i, &item as *const _ as isize);
                    }
                    strip.signature = signature;
                }
                if SendMessageW(strip.tabs, TCM_GETCURSEL, 0, 0) != group.active as isize {
                    SendMessageW(strip.tabs, TCM_SETCURSEL, group.active, 0);
                }
                // Strip controls have their own monitor DPI, independently of the manager.
                let height = group.strip_height;
                if strip.height != height {
                    let font = CreateFontW(
                        -(15 * height / 42),
                        0,
                        0,
                        0,
                        FW_NORMAL as i32,
                        0,
                        0,
                        0,
                        DEFAULT_CHARSET as u32,
                        OUT_DEFAULT_PRECIS as u32,
                        CLIP_DEFAULT_PRECIS as u32,
                        CLEARTYPE_QUALITY as u32,
                        DEFAULT_PITCH as u32,
                        w!("Segoe UI"),
                    );
                    if !font.is_null() {
                        for control in [
                            strip.tabs,
                            GetDlgItem(strip.window, STRIP_MENU),
                            GetDlgItem(strip.window, STRIP_CLOSE),
                        ] {
                            SendMessageW(control, WM_SETFONT, font as usize, 1);
                        }
                        if !strip.font.is_null() {
                            DeleteObject(strip.font);
                        }
                        strip.font = font;
                        strip.height = height;
                    }
                }
                let width = group.bounds.width();
                let button = (height - 4).max(24);
                if strip
                    .geometry
                    .is_none_or(|(_, _, w, h)| w != width || h != height)
                {
                    MoveWindow(
                        strip.tabs,
                        2,
                        2,
                        (width - 2 * button - 6).max(20),
                        height - 4,
                        1,
                    );
                    MoveWindow(
                        GetDlgItem(strip.window, STRIP_MENU),
                        width - 2 * button - 3,
                        2,
                        button,
                        height - 4,
                        1,
                    );
                    MoveWindow(
                        GetDlgItem(strip.window, STRIP_CLOSE),
                        width - button - 2,
                        2,
                        button,
                        height - 4,
                        1,
                    );
                }
                let work =
                    platform::work_area(platform::hwnd(&group.members[group.active].identity));
                let y = (group.bounds.top - height).max(work.top);
                // Put the strip immediately above its active target, never globally topmost.
                let target = platform::hwnd(&group.members[group.active].identity);
                let previous = GetWindow(target, GW_HWNDPREV);
                // Inserting into the topmost band could promote the strip. Stay in the ordinary band.
                let insert_after = if !previous.is_null()
                    && GetWindowLongPtrW(previous, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0
                {
                    HWND_TOP
                } else {
                    previous
                };
                let geometry = (group.bounds.left, y, width, height);
                if strip.geometry != Some(geometry) || !strip.shown || previous != strip.window {
                    SetWindowPos(
                        strip.window,
                        insert_after,
                        group.bounds.left,
                        y,
                        width,
                        height,
                        SWP_NOACTIVATE
                            | if previous == strip.window {
                                SWP_NOZORDER
                            } else {
                                0
                            }
                            | if group.shown {
                                SWP_SHOWWINDOW
                            } else {
                                SWP_HIDEWINDOW
                            },
                    );
                    strip.geometry = Some(geometry);
                    strip.shown = true;
                }
            }
        }
    }
    fn refresh_hooks(&mut self) {
        if self.capture {
            return;
        }
        EVENT_WINDOWS.with(|targets| {
            *targets.borrow_mut() = self
                .snapshot
                .groups
                .iter()
                .flat_map(|g| {
                    g.members
                        .iter()
                        .map(|w| platform::hwnd(&w.identity) as isize)
                })
                .collect()
        });
        let pids: BTreeSet<_> = self
            .snapshot
            .groups
            .iter()
            .flat_map(|g| g.members.iter().map(|w| w.identity.pid))
            .collect();
        if pids == self.hook_pids {
            return;
        }
        for hook in self.hooks.drain(..) {
            unsafe {
                UnhookWinEvent(hook);
            }
        }
        self.hook_pids = pids;
        for pid in &self.hook_pids {
            for (first, last) in [
                (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
                (EVENT_OBJECT_LOCATIONCHANGE, EVENT_OBJECT_NAMECHANGE),
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                (EVENT_OBJECT_CLOAKED, EVENT_OBJECT_UNCLOAKED),
            ] {
                let hook = unsafe {
                    SetWinEventHook(
                        first,
                        last,
                        null_mut(),
                        Some(event_proc),
                        *pid,
                        0,
                        WINEVENT_OUTOFCONTEXT | WINEVENT_SKIPOWNPROCESS,
                    )
                };
                if !hook.is_null() {
                    self.hooks.push(hook);
                } else {
                    self.set(ID_STATUS,"Window event subscription failed. Use Refresh windows to synchronize groups.");
                }
            }
        }
        if !self.snapshot.quit {
            self.send(Command::Sync);
        }
    }
    fn check_update(&mut self) {
        if self.capture {
            return;
        }
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        self.set(
            ID_UPDATE_STATUS,
            &format!("Installed version: {}. Checking GitHub…", crate::VERSION),
        );
        unsafe {
            EnableWindow(self.controls[&ID_CHECK], 0);
            EnableWindow(self.controls[&ID_INSTALL], 0);
        }
        let mailbox = self.update_mailbox.clone();
        let cancel = self.cancel.clone();
        let window = self.window as isize;
        thread::spawn(move || {
            post_update(
                &mailbox,
                window,
                UpdateNotice::Checked(update::native::check(&cancel)),
            );
        });
    }
    fn download_update(&mut self) {
        if self.capture {
            return;
        }
        if self.update_busy {
            return;
        }
        if self.snapshot.recovery > 0 {
            self.set(
                ID_UPDATE_STATUS,
                "Recover the previous session before downloading and installing an update.",
            );
            return;
        }
        let Some(available) = self.available.clone() else {
            return;
        };
        self.update_busy = true;
        self.set(ID_UPDATE_STATUS,"Downloading and verifying the update. WindowTabs will restore groups and restart after installation.");
        unsafe {
            EnableWindow(self.controls[&ID_CHECK], 0);
            EnableWindow(self.controls[&ID_INSTALL], 0);
        }
        let mailbox = self.update_mailbox.clone();
        let cancel = self.cancel.clone();
        let window = self.window as isize;
        let root = self.store.root.clone();
        thread::spawn(move || {
            let mut last = 0;
            let result = update::native::download(&available, &root, cancel, |bytes| {
                let percent = bytes * 100 / available.executable.size;
                if percent != last {
                    last = percent;
                    post_update(
                        &mailbox,
                        window,
                        UpdateNotice::Progress(bytes, available.executable.size),
                    );
                }
            });
            post_update(&mailbox, window, UpdateNotice::Downloaded(result));
        });
    }
    fn update_notice(&mut self) {
        let notice = self
            .update_mailbox
            .lock()
            .ok()
            .and_then(|mut output| output.take());
        match notice {
            Some(UpdateNotice::Checked(result))=>{
                self.update_busy=false;
                match result {
                    Ok(Some(available))=>{self.set(ID_UPDATE_STATUS,&format!("Installed: {}. Version {} is available. Download and install will restart WindowTabs.",crate::VERSION,available.version));self.available=Some(available);},
                    Ok(None)=>{self.set(ID_UPDATE_STATUS,&format!("Installed version: {}. You are up to date on the stable channel.",crate::VERSION));self.available=None;},
                    Err(error)=>self.set(ID_UPDATE_STATUS,&format!("Installed: {}. Check failed: {error}",crate::VERSION)),
                }
            },
            Some(UpdateNotice::Progress(bytes,total))=>self.set(ID_UPDATE_STATUS,&format!("Downloading update: {}% ({} / {} KiB). The installed EXE is unchanged until verification succeeds.",bytes*100/total,bytes/1024,total/1024)),
            Some(UpdateNotice::Downloaded(result))=>{
                self.update_busy=false;
                match result {
                    Ok(plan)=>{self.install_request_id+=1;self.install_plan=Some(plan);self.set(ID_UPDATE_STATUS,"Verified. Restoring groups before installation and restart…");self.closing=true;self.send(Command::InstallAndQuit(self.install_request_id));},
                    Err(error)=>self.set(ID_UPDATE_STATUS,&format!("Installation failed: {error}")),
                }
            },
            None=>{},
        }
        unsafe {
            EnableWindow(self.controls[&ID_CHECK], i32::from(!self.update_busy));
            EnableWindow(
                self.controls[&ID_INSTALL],
                i32::from(
                    !self.update_busy && self.available.is_some() && self.install_plan.is_none(),
                ),
            );
        }
    }
    fn strip_action(&mut self, id: GroupId, action: isize) {
        let Some(group) = self.snapshot.groups.iter().find(|g| g.id == id) else {
            return;
        };
        let active = group.active;
        let count = group.members.len();
        match action {
            1 => self.send(Command::Close(id)),
            2 => self.send(Command::Detach(id, active)),
            3 => self.send(Command::Reorder(id, active, active.saturating_sub(1))),
            4 => self.send(Command::Reorder(id, active, (active + 1).min(count - 1))),
            5 => self.send(Command::Cycle(id, -1)),
            6 => self.send(Command::Cycle(id, 1)),
            _ => unsafe {
                ShowWindow(self.window, SW_RESTORE);
                SetForegroundWindow(self.window);
            },
        }
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        unsafe {
            for hook in self.hooks.drain(..) {
                UnhookWinEvent(hook);
            }
            for (_, strip) in self.strips.drain() {
                DestroyWindow(strip.window);
                if !strip.font.is_null() {
                    DeleteObject(strip.font);
                }
            }
            for object in [
                self.font,
                self.title_font,
                self.heading_font,
                self.detail_font,
            ] {
                if !object.is_null() {
                    DeleteObject(object);
                }
            }
        }
    }
}

fn selected(list: HWND) -> Option<usize> {
    let index = unsafe { SendMessageW(list, LB_GETCURSEL, 0, 0) };
    (index >= 0).then_some(index as usize)
}
fn list_fill(list: HWND, items: impl Iterator<Item = String>) {
    unsafe {
        // DefWindowProc adds WS_VISIBLE when redraw is reenabled. Never enable
        // redraw on an off-page list, or a background snapshot can reveal it.
        let shown = GetWindowLongPtrW(list, GWL_STYLE) as u32 & WS_VISIBLE != 0;
        if shown {
            SendMessageW(list, WM_SETREDRAW, 0, 0);
        }
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for item in items {
            let text = wide(&item);
            SendMessageW(list, LB_ADDSTRING, 0, text.as_ptr() as isize);
        }
        if shown {
            SendMessageW(list, WM_SETREDRAW, 1, 0);
            InvalidateRect(list, null(), 1);
        }
    }
}
fn post_update(mailbox: &Arc<Mutex<Option<UpdateNotice>>>, window: isize, notice: UpdateNotice) {
    if let Ok(mut output) = mailbox.lock() {
        *output = Some(notice);
    }
    unsafe {
        PostMessageW(window as HWND, UPDATE, 0, 0);
    }
}

unsafe extern "system" fn event_proc(
    _: HWINEVENTHOOK,
    event: u32,
    window: HWND,
    object: i32,
    child: i32,
    _: u32,
    _: u32,
) {
    if event >= EVENT_OBJECT_CREATE && (object != OBJID_WINDOW || child != 0) {
        return;
    }
    if !EVENT_WINDOWS.with(|targets| {
        targets
            .try_borrow()
            .is_ok_and(|targets| targets.contains(&(window as isize)))
    }) {
        return;
    }
    let manager = MANAGER.load(Ordering::Relaxed);
    if manager != 0 && !EVENT_PENDING.swap(true, Ordering::Relaxed) {
        unsafe {
            PostMessageW(manager as HWND, EVENTS, 0, 0);
        }
    }
}

unsafe extern "system" fn main_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_DRAWITEM {
        return unsafe { art::draw(lparam as *const DRAWITEMSTRUCT) };
    }
    if message == WM_MEASUREITEM {
        let measure = unsafe { &mut *(lparam as *mut MEASUREITEMSTRUCT) };
        if measure.CtlType == ODT_LISTBOX {
            measure.itemHeight = 54 * unsafe { GetDpiForWindow(window) }.max(96) / 96;
            return 1;
        }
    }
    // Control painting can reenter while a view is being updated. Keep it independent of App's mutable borrow.
    if message == WM_CTLCOLORSTATIC
        || message == WM_CTLCOLOREDIT
        || message == WM_CTLCOLORLISTBOX
        || message == WM_CTLCOLORBTN
    {
        let colors = palette();
        let dc = wparam as HDC;
        unsafe {
            let id = GetDlgCtrlID(lparam as HWND);
            SetTextColor(
                dc,
                if [ID_TAGLINE, ID_HELP, ID_EMPTY_HELP, ID_STATUS, ID_PICK_COUNT].contains(&id) {
                    colors.secondary
                } else {
                    colors.text
                },
            );
            let background = if message == WM_CTLCOLOREDIT || message == WM_CTLCOLORLISTBOX {
                colors.surface
            } else {
                colors.background
            };
            SetBkColor(dc, background);
            SetDCBrushColor(dc, background);
            return GetStockObject(DC_BRUSH) as isize;
        }
    }
    if message == WM_ERASEBKGND || message == WM_PRINTCLIENT {
        paint_background(window, wparam as HDC);
        return 1;
    }
    if message == WM_PAINT {
        unsafe {
            let mut paint: PAINTSTRUCT = zeroed();
            let dc = BeginPaint(window, &mut paint);
            paint_background(window, dc);
            EndPaint(window, &paint);
        }
        return 0;
    }
    if message == WM_DESTROY {
        unsafe {
            PostQuitMessage(0);
        }
        return 0;
    }
    let handled = APP.with(|state| {
        let Ok(mut state) = state.try_borrow_mut() else {
            return None;
        };
        let app = state.as_mut()?;
        match message {
            DM_GETDEFID => {
                let id = match app.page {
                    Page::Picker(_) => ID_CREATE,
                    Page::Saved => ID_RESTORE,
                    Page::Groups => ID_SWITCH,
                    Page::Settings => ID_CHECK,
                };
                Some(if unsafe { IsWindowEnabled(app.controls[&id]) } != 0 {
                    ((0x534b_u32 << 16) | id as u32) as isize
                } else {
                    0
                })
            }
            WM_SIZE => {
                app.render_manager();
                app.layout();
                Some(0)
            }
            WM_GETMINMAXINFO => {
                let info = unsafe { &mut *(lparam as *mut MINMAXINFO) };
                let mut minimum = RECT {
                    left: 0,
                    top: 0,
                    right: app.scale(MIN_WIDTH),
                    bottom: app.scale(MIN_HEIGHT),
                };
                unsafe {
                    AdjustWindowRectExForDpi(
                        &mut minimum,
                        GetWindowLongPtrW(window, GWL_STYLE) as u32,
                        0,
                        GetWindowLongPtrW(window, GWL_EXSTYLE) as u32,
                        app.dpi() as u32,
                    );
                }
                info.ptMinTrackSize = POINT {
                    x: minimum.right - minimum.left,
                    y: minimum.bottom - minimum.top,
                };
                Some(0)
            }
            WM_SHOWWINDOW if wparam != 0 => {
                app.render_manager();
                Some(0)
            }
            WM_COMMAND => {
                app.command((wparam & 0xffff) as i32, ((wparam >> 16) & 0xffff) as u16);
                Some(0)
            }
            STATE => {
                app.state();
                Some(0)
            }
            EVENTS => {
                unsafe {
                    SetTimer(window, EVENT_TIMER, 75, None);
                }
                Some(0)
            }
            WM_TIMER if wparam == EVENT_TIMER => {
                unsafe {
                    KillTimer(window, EVENT_TIMER);
                }
                EVENT_PENDING.store(false, Ordering::Relaxed);
                app.send(Command::Sync);
                Some(0)
            }
            TAB => {
                app.send(Command::Activate(wparam as u64, lparam as usize));
                Some(0)
            }
            STRIP_ACTION => {
                app.strip_action(wparam as u64, lparam);
                Some(0)
            }
            UPDATE => {
                app.update_notice();
                Some(0)
            }
            WM_CLOSE => {
                if !app.snapshot.ready {
                    unsafe {
                        DestroyWindow(window);
                    }
                } else if !app.closing {
                    app.closing = true;
                    app.send(Command::Quit);
                }
                Some(0)
            }
            WM_DPICHANGED => {
                let rect = unsafe { &*(lparam as *const RECT) };
                unsafe {
                    SetWindowPos(
                        window,
                        null_mut(),
                        rect.left,
                        rect.top,
                        rect.right - rect.left,
                        rect.bottom - rect.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                app.update_font();
                app.layout_key = None;
                app.list_cache = ContentCache::default();
                app.dirty_manager = true;
                app.render_manager();
                app.layout();
                Some(0)
            }
            WM_QUERYENDSESSION => Some(1), // The durable journal covers forced session termination.
            _ => None,
        }
    });
    handled.unwrap_or_else(|| unsafe { DefWindowProcW(window, message, wparam, lparam) })
}

unsafe extern "system" fn strip_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let manager = MANAGER.load(Ordering::Relaxed) as HWND;
    let group = unsafe { GetWindowLongPtrW(window, GWLP_USERDATA) } as usize;
    match message {
        WM_NOTIFY => {
            let notification = unsafe { &*(lparam as *const NMHDR) };
            if notification.code == TCN_SELCHANGE {
                let index =
                    unsafe { SendMessageW(GetDlgItem(window, STRIP_TABS), TCM_GETCURSEL, 0, 0) };
                if index >= 0 {
                    unsafe {
                        PostMessageW(manager, TAB, group, index);
                    }
                }
            }
            0
        }
        WM_COMMAND => {
            match (wparam & 0xffff) as i32 {
                STRIP_CLOSE => unsafe {
                    PostMessageW(manager, STRIP_ACTION, group, 1);
                },
                STRIP_MENU => unsafe {
                    strip_menu(window, manager, group);
                },
                _ => {}
            }
            0
        }
        WM_CONTEXTMENU => {
            unsafe {
                strip_menu(window, manager, group);
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                PostMessageW(manager, STRIP_ACTION, group, 1);
            }
            0
        }
        WM_DPICHANGED => {
            if !EVENT_PENDING.swap(true, Ordering::Relaxed) {
                unsafe {
                    PostMessageW(manager, EVENTS, 0, 0);
                }
            }
            0
        }
        _ => unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
}
unsafe fn strip_menu(window: HWND, manager: HWND, group: usize) {
    unsafe {
        let menu = CreatePopupMenu();
        for (id, label) in [
            (5, "Previous tab"),
            (6, "Next tab"),
            (3, "Move active tab left"),
            (4, "Move active tab right"),
            (2, "Detach active tab"),
            (7, "Show manager"),
            (1, "Close group and restore windows"),
        ] {
            AppendMenuW(menu, MF_STRING, id, wide(label).as_ptr());
        }
        let mut point = POINT::default();
        GetCursorPos(&mut point);
        let action = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            0,
            window,
            null(),
        );
        DestroyMenu(menu);
        if action > 0 {
            PostMessageW(manager, STRIP_ACTION, group, action as isize);
        }
    }
}
