//! Native controls and a blocking GetMessage loop. All disk/network work is off-thread.
use crate::{
    controller::{self, Command, GroupView, Snapshot, Worker},
    model::*,
    platform::{self, wide},
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
        System::LibraryLoader::*,
        UI::{
            Accessibility::*, Controls::*, HiDpi::*, Input::KeyboardAndMouse::*,
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

const MAIN_CLASS: *const u16 = w!("WindowTabs.Manager.v1");
const STRIP_CLASS: *const u16 = w!("WindowTabs.Strip.v1");
const STATE: u32 = WM_APP + 1;
const EVENTS: u32 = WM_APP + 2;
const TAB: u32 = WM_APP + 3;
const STRIP_ACTION: u32 = WM_APP + 4;
const UPDATE: u32 = WM_APP + 5;
const EVENT_TIMER: usize = 1;
const ID_WINDOWS: i32 = 100;
const ID_NAME: i32 = 101;
const ID_GROUPS: i32 = 102;
const ID_MEMBERS: i32 = 103;
const ID_SAVED: i32 = 104;
const ID_REFRESH: i32 = 110;
const ID_CREATE: i32 = 111;
const ID_ADD: i32 = 112;
const ID_RENAME: i32 = 113;
const ID_SWITCH: i32 = 114;
const ID_LEFT: i32 = 115;
const ID_RIGHT: i32 = 116;
const ID_DETACH: i32 = 117;
const ID_CLOSE: i32 = 118;
const ID_SAVE: i32 = 119;
const ID_RESTORE: i32 = 120;
const ID_DELETE: i32 = 121;
const ID_RECOVER: i32 = 122;
const ID_STARTUP: i32 = 123;
const ID_CHECK: i32 = 124;
const ID_INSTALL: i32 = 125;
const ID_STATUS: i32 = 130;
const ID_UPDATE_STATUS: i32 = 131;
const ID_LABEL_WINDOWS: i32 = 140;
const ID_LABEL_GROUPS: i32 = 141;
const ID_LABEL_MEMBERS: i32 = 142;
const ID_LABEL_NAME: i32 = 143;
const ID_LABEL_SAVED: i32 = 144;
const ID_LABEL_SETTINGS: i32 = 145;
const STRIP_TABS: i32 = 200;
const STRIP_MENU: i32 = 201;
const STRIP_CLOSE: i32 = 202;

static MANAGER: AtomicIsize = AtomicIsize::new(0);
static EVENT_PENDING: AtomicBool = AtomicBool::new(false);
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
                hIcon: LoadIconW(null_mut(), IDI_APPLICATION),
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
            (1120 * dpi / 96).min((work.right - work.left).max(950)),
            (850 * dpi / 96).min((work.bottom - work.top).max(700)),
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
        let app = App::new(window, store)?;
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
    unsafe fn new(window: HWND, store: Store) -> Result<Self> {
        let worker = controller::start(store.clone(), window as isize, STATE)?;
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
        };
        unsafe {
            for (id, text) in [
                (
                    ID_LABEL_WINDOWS,
                    "1. Select windows (Ctrl / Shift for multiple)",
                ),
                (ID_LABEL_GROUPS, "2. Active groups"),
                (ID_LABEL_MEMBERS, "Members — double-click to switch"),
                (ID_LABEL_NAME, "Group name"),
                (
                    ID_LABEL_SAVED,
                    "Saved groups — open matching windows before restoring",
                ),
                (ID_LABEL_SETTINGS, "Settings / Updates"),
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
                (ID_REFRESH, "Refresh windows"),
                (ID_CREATE, "Create group"),
                (ID_ADD, "Add to group"),
                (ID_RENAME, "Rename"),
                (ID_SWITCH, "Switch to tab"),
                (ID_LEFT, "Move left"),
                (ID_RIGHT, "Move right"),
                (ID_DETACH, "Detach"),
                (ID_CLOSE, "Close group"),
                (ID_SAVE, "Save group"),
                (ID_RESTORE, "Restore saved"),
                (ID_DELETE, "Delete saved"),
                (ID_RECOVER, "Recover previous session"),
                (ID_CHECK, "Check for updates"),
                (ID_INSTALL, "Download and install"),
            ] {
                app.control(id, "BUTTON", text, WS_TABSTOP | BS_PUSHBUTTON as u32)?;
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
    fn scale(&self, value: i32) -> i32 {
        value * unsafe { GetDpiForWindow(self.window) }.max(96) as i32 / 96
    }
    fn layout(&mut self) {
        unsafe {
            let mut client = RECT::default();
            GetClientRect(self.window, &mut client);
            let dpi = GetDpiForWindow(self.window).max(96) as i32;
            let width = client.right * 96 / dpi;
            let height = client.bottom * 96 / dpi;
            let margin = 18;
            let gap = 16;
            let half = (width - 2 * margin - gap) / 2;
            let right = margin + half + gap;
            let top = 44;
            let list_height = (height - 445).max(160);
            let bottom = top + list_height;
            let place = |id: i32, x: i32, y: i32, w: i32, h: i32| {
                MoveWindow(
                    self.controls[&id],
                    self.scale(x),
                    self.scale(y),
                    self.scale(w.max(1)),
                    self.scale(h.max(1)),
                    1,
                );
            };
            place(ID_LABEL_WINDOWS, margin, 16, half, 24);
            place(ID_WINDOWS, margin, top, half, list_height);
            place(ID_LABEL_GROUPS, right, 16, half, 24);
            place(ID_GROUPS, right, top, half, 72);
            place(ID_LABEL_MEMBERS, right, top + 82, half, 24);
            place(ID_MEMBERS, right, top + 108, half, list_height - 108);
            place(ID_REFRESH, margin, bottom + 9, 142, 30);
            place(ID_ADD, margin + 152, bottom + 9, 140, 30);
            let unit = (half - 24) / 4;
            for (n, id) in [ID_SWITCH, ID_LEFT, ID_RIGHT, ID_DETACH].iter().enumerate() {
                place(*id, right + n as i32 * (unit + 8), bottom + 9, unit, 30);
            }
            place(ID_LABEL_NAME, margin, bottom + 50, half, 20);
            place(ID_NAME, margin, bottom + 73, half - 144, 29);
            place(ID_CREATE, margin + half - 134, bottom + 73, 134, 29);
            for (n, id) in [ID_RENAME, ID_SAVE, ID_CLOSE].iter().enumerate() {
                place(
                    *id,
                    right + n as i32 * ((half - 16) / 3 + 8),
                    bottom + 60,
                    (half - 16) / 3,
                    32,
                );
            }
            let lower = bottom + 119;
            place(ID_LABEL_SAVED, margin, lower, half, 24);
            place(ID_SAVED, margin, lower + 27, half, 86);
            place(ID_RESTORE, margin, lower + 121, 130, 30);
            place(ID_DELETE, margin + 140, lower + 121, 130, 30);
            place(ID_LABEL_SETTINGS, right, lower, half, 24);
            place(ID_STARTUP, right, lower + 26, half, 27);
            place(ID_CHECK, right, lower + 60, (half - 10) / 2, 31);
            place(
                ID_INSTALL,
                right + (half + 10) / 2,
                lower + 60,
                (half - 10) / 2,
                31,
            );
            place(ID_UPDATE_STATUS, right, lower + 99, half, 70);
            place(ID_RECOVER, margin, height - 85, 225, 30);
            place(ID_STATUS, margin, height - 47, width - 2 * margin, 43);
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
            }
        }
    }
    fn set(&self, id: i32, text: &str) {
        unsafe {
            SetWindowTextW(self.controls[&id], wide(text).as_ptr());
        }
    }
    fn send(&mut self, command: Command) {
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
                .filter_map(|i| {
                    self.snapshot
                        .windows
                        .get(i as usize)
                        .map(|w| w.identity.clone())
                })
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
            self.members(group, None);
        }
    }
    fn members(&self, group: &GroupView, prefer: Option<Identity>) {
        let list = self.controls[&ID_MEMBERS];
        list_fill(
            list,
            group.members.iter().enumerate().map(|(i, w)| {
                format!("{}{}", if i == group.active { "● " } else { "  " }, w.title)
            }),
        );
        let index = prefer
            .and_then(|id| group.members.iter().position(|w| w.identity == id))
            .unwrap_or(group.active);
        unsafe {
            SendMessageW(list, LB_SETCURSEL, index, 0);
        }
    }
    fn command(&mut self, id: i32, notification: u16) {
        if id == ID_GROUPS && notification == LBN_SELCHANGE as u16 {
            self.select_group();
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
            ID_REFRESH => {
                self.hook_pids.clear();
                self.send(Command::Refresh);
            }
            ID_CREATE => self.send(Command::Create(self.name(), self.selected_windows())),
            ID_RECOVER => self.send(Command::Recover),
            ID_STARTUP => self.send(Command::Startup(
                unsafe { SendMessageW(self.controls[&ID_STARTUP], BM_GETCHECK, 0, 0) }
                    == BST_CHECKED as isize,
            )),
            ID_CHECK => self.check_update(),
            ID_INSTALL => self.download_update(),
            ID_RESTORE | ID_DELETE => {
                if let Some(index) = selected(self.controls[&ID_SAVED]) {
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
                        ID_ADD => self.send(Command::Add(group, self.selected_windows())),
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
        let selected_group = self.selected_group();
        let selected_member = selected_group
            .and_then(|id| self.snapshot.groups.iter().find(|g| g.id == id))
            .and_then(|g| self.member().and_then(|i| g.members.get(i)))
            .map(|w| w.identity.clone());
        let selected_windows = self.selected_windows();
        let saved = selected(self.controls[&ID_SAVED]);
        let windows_changed = self
            .snapshot
            .windows
            .iter()
            .map(|w| (&w.identity, &w.title))
            .collect::<Vec<_>>()
            != snapshot
                .windows
                .iter()
                .map(|w| (&w.identity, &w.title))
                .collect::<Vec<_>>();
        self.snapshot = snapshot;
        if windows_changed {
            list_fill(
                self.controls[&ID_WINDOWS],
                self.snapshot.windows.iter().map(|w| {
                    format!(
                        "{}   [{}]",
                        w.title,
                        std::path::Path::new(&w.executable)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )
                }),
            );
            for (i, window) in self.snapshot.windows.iter().enumerate() {
                if selected_windows.contains(&window.identity) {
                    unsafe {
                        SendMessageW(self.controls[&ID_WINDOWS], LB_SETSEL, 1, i as isize);
                    }
                }
            }
        }
        self.group_ids = self.snapshot.groups.iter().map(|g| g.id).collect();
        list_fill(
            self.controls[&ID_GROUPS],
            self.snapshot.groups.iter().map(|g| {
                format!(
                    "{}  ·  {} tabs{}",
                    g.name,
                    g.members.len(),
                    if g.restoring {
                        " (recovery required)"
                    } else if g.shown {
                        ""
                    } else {
                        " (minimized / hidden)"
                    }
                )
            }),
        );
        if !self.group_ids.is_empty() {
            let index = selected_group
                .and_then(|id| self.group_ids.iter().position(|i| *i == id))
                .unwrap_or(0);
            unsafe {
                SendMessageW(self.controls[&ID_GROUPS], LB_SETCURSEL, index, 0);
            }
            self.members(&self.snapshot.groups[index], selected_member);
            if selected_group != Some(self.group_ids[index]) {
                self.set(ID_NAME, &self.snapshot.groups[index].name);
            }
        } else {
            list_fill(self.controls[&ID_MEMBERS], std::iter::empty::<String>());
        }
        list_fill(
            self.controls[&ID_SAVED],
            self.snapshot
                .settings
                .templates
                .iter()
                .map(|t| format!("{}  ·  {} windows", t.name, t.members.len())),
        );
        if let Some(index) = saved
            && index < self.snapshot.settings.templates.len()
        {
            unsafe {
                SendMessageW(self.controls[&ID_SAVED], LB_SETCURSEL, index, 0);
            }
        }
        unsafe {
            SendMessageW(
                self.controls[&ID_STARTUP],
                BM_SETCHECK,
                if self.snapshot.settings.check_on_startup {
                    BST_CHECKED
                } else {
                    BST_UNCHECKED
                } as usize,
                0,
            );
            EnableWindow(
                self.controls[&ID_RECOVER],
                i32::from(self.snapshot.recovery > 0),
            );
        }
        if self.snapshot.recovery > 0 {
            self.set(ID_STATUS,&format!("Recovery required: {} previous-session windows. Choose Recover previous session before grouping windows.",self.snapshot.recovery));
        } else {
            self.set(ID_STATUS, &self.snapshot.status);
        }
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
    fn refresh_strips(&mut self) {
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
                    }
                });
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
                SendMessageW(strip.tabs, TCM_SETCURSEL, group.active, 0);
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
            }
        }
    }
    fn refresh_hooks(&mut self) {
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
            if !self.font.is_null() {
                DeleteObject(self.font);
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
        SendMessageW(list, WM_SETREDRAW, 0, 0);
        SendMessageW(list, LB_RESETCONTENT, 0, 0);
        for item in items {
            SendMessageW(list, LB_ADDSTRING, 0, wide(&item).as_ptr() as isize);
        }
        SendMessageW(list, LB_SETHORIZONTALEXTENT, 1400, 0);
        SendMessageW(list, WM_SETREDRAW, 1, 0);
        InvalidateRect(list, null(), 1);
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
            WM_SIZE => {
                app.layout();
                Some(0)
            }
            WM_GETMINMAXINFO => {
                let info = unsafe { &mut *(lparam as *mut MINMAXINFO) };
                info.ptMinTrackSize = POINT {
                    x: app.scale(950),
                    y: app.scale(700),
                };
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
