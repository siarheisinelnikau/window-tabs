//! A single bounded worker owns persistence, discovery and group state.
use crate::{
    model::*,
    platform::{self, NativeAdapter},
    storage::{Settings, Store},
};
use std::{
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
    thread,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    IsIconic, IsWindowVisible, IsZoomed, PostMessageW,
};

#[derive(Clone)]
pub struct GroupView {
    pub id: GroupId,
    pub name: String,
    pub members: Vec<WindowInfo>,
    pub active: usize,
    pub bounds: Rect,
    pub strip_height: i32,
    pub shown: bool,
}
#[derive(Clone, Default)]
pub struct Snapshot {
    pub windows: Vec<WindowInfo>,
    pub groups: Vec<GroupView>,
    pub settings: Settings,
    pub recovery: usize,
    pub status: String,
    pub ready: bool,
    pub quit: bool,
}
pub enum Command {
    Refresh,
    Sync,
    Create(String, Vec<Identity>),
    Add(GroupId, Vec<Identity>),
    Rename(GroupId, String),
    Activate(GroupId, usize),
    Cycle(GroupId, isize),
    Reorder(GroupId, usize, usize),
    Detach(GroupId, usize),
    Close(GroupId),
    Save(GroupId),
    Restore(usize),
    Delete(usize),
    Startup(bool),
    Recover,
    Quit,
}
pub struct Worker {
    pub sender: SyncSender<Command>,
    pub mailbox: Arc<Mutex<Option<Snapshot>>>,
}

struct Controller {
    engine: Groups<NativeAdapter>,
    settings: Settings,
    recovery: Vec<Original>,
    candidates: Vec<WindowInfo>,
}
impl Controller {
    fn open(store: Store) -> Result<Self> {
        let settings = store.settings()?;
        let recovery = store.recovery()?;
        Ok(Self {
            engine: Groups::new(NativeAdapter { store }),
            settings,
            recovery,
            candidates: vec![],
        })
    }
    fn refresh(&mut self) {
        self.candidates = platform::discover()
            .into_iter()
            .filter(|w| !self.engine.contains(&w.identity))
            .collect();
    }
    fn bounds(&self, identity: &Identity) -> Rect {
        let window = platform::hwnd(identity);
        let initial = self
            .candidates
            .iter()
            .find(|w| &w.identity == identity)
            .map(|w| w.bounds)
            .or_else(|| platform::rect(window))
            .unwrap_or_default();
        initial.fit(platform::work_area(window), platform::strip_height(window))
    }
    fn action(&mut self, command: Command) -> Result<(String, bool)> {
        let modifies = !matches!(
            command,
            Command::Refresh
                | Command::Sync
                | Command::Recover
                | Command::Startup(_)
                | Command::Delete(_)
                | Command::Quit
        );
        if modifies && !self.recovery.is_empty() {
            return Err("Recover the previous session before creating or changing groups.".into());
        }
        let mut quit = false;
        let text = match command {
            Command::Refresh => {
                self.engine.prune()?;
                self.refresh();
                "Eligible windows refreshed. Use Ctrl or Shift to select several windows.".into()
            }
            Command::Sync => {
                self.synchronize()?;
                String::new()
            }
            Command::Create(name, windows) => {
                let first = windows
                    .first()
                    .ok_or("Select at least two eligible windows.")?;
                let bounds = self.bounds(first);
                self.engine.create(&name, &windows, bounds)?;
                self.refresh();
                "Group created. Use its tab strip or the member list to switch windows.".into()
            }
            Command::Add(id, windows) => {
                self.engine.add(id, &windows)?;
                self.refresh();
                "Windows added to the selected group.".into()
            }
            Command::Rename(id, name) => {
                self.engine.rename(id, &name)?;
                "Group renamed. Save it to update its saved definition.".into()
            }
            Command::Activate(id, tab) => {
                self.engine.activate(id, tab, true)?;
                String::new()
            }
            Command::Cycle(id, delta) => {
                let g = self.engine.get(id).ok_or("Select an active group.")?;
                let next =
                    (g.active as isize + delta).rem_euclid(g.members.len() as isize) as usize;
                self.engine.activate(id, next, true)?;
                String::new()
            }
            Command::Reorder(id, from, to) => {
                self.engine.reorder(id, from, to)?;
                "Tab order updated. Save the group to keep this order.".into()
            }
            Command::Detach(id, tab) => {
                self.engine.detach(id, tab)?;
                self.refresh();
                "Window detached and its original placement restored.".into()
            }
            Command::Close(id) => {
                self.engine.close(id)?;
                self.refresh();
                "Group closed. Original placements and show states restored.".into()
            }
            Command::Save(id) => {
                let group = self.engine.get(id).ok_or("Select an active group.")?;
                if group.members.len() < 2 {
                    return Err("Saved groups require at least two members.".into());
                }
                let mut template = Template::from_group(group);
                for (selector, member) in template.members.iter_mut().zip(&group.members) {
                    let title = platform::title(platform::hwnd(&member.info.identity));
                    if !title.is_empty() {
                        selector.title = title;
                    }
                }
                let mut settings = self.settings.clone();
                if let Some(old) = settings
                    .templates
                    .iter_mut()
                    .find(|t| t.name == template.name)
                {
                    *old = template;
                } else {
                    if settings.templates.len() >= 64 {
                        return Err("The limit is 64 saved groups.".into());
                    }
                    settings.templates.push(template);
                }
                self.engine.adapter.store.save_settings(&settings)?;
                self.settings = settings;
                "Group definition saved. Restore matches open windows by application, class and exact title.".into()
            }
            Command::Restore(index) => {
                self.refresh();
                let template = self
                    .settings
                    .templates
                    .get(index)
                    .ok_or("Select a saved group.")?;
                let identities = template.resolve(&self.candidates)?;
                let name = template.name.clone();
                let bounds = self.bounds(&identities[0]);
                self.engine.create(&name, &identities, bounds)?;
                self.refresh();
                "Saved group restored using already open windows.".into()
            }
            Command::Delete(index) => {
                let mut settings = self.settings.clone();
                if index >= settings.templates.len() {
                    return Err("Select a saved group.".into());
                }
                settings.templates.remove(index);
                self.engine.adapter.store.save_settings(&settings)?;
                self.settings = settings;
                "Saved definition deleted. Open application windows were not closed.".into()
            }
            Command::Startup(enabled) => {
                let mut settings = self.settings.clone();
                settings.check_on_startup = enabled;
                self.engine.adapter.store.save_settings(&settings)?;
                self.settings = settings;
                "Update preference saved.".into()
            }
            Command::Recover => {
                let mut remaining = vec![];
                for original in &self.recovery {
                    if self.engine.adapter.alive(&original.info.identity)
                        && self.engine.adapter.restore(original).is_err()
                    {
                        remaining.push(original.clone());
                    }
                }
                self.recovery = remaining;
                self.engine.adapter.store.save_recovery(&self.recovery)?;
                self.refresh();
                if !self.recovery.is_empty() {
                    return Err(format!(
                        "{} windows could not be restored. Close modal dialogs or restart the affected application, then retry recovery.",
                        self.recovery.len()
                    ));
                }
                "Previous session recovered. Missing or exited processes were skipped.".into()
            }
            Command::Quit => {
                self.engine.close_all()?;
                quit = true;
                "Groups restored. Closing WindowTabs.".into()
            }
        };
        Ok((text, quit))
    }
    fn synchronize(&mut self) -> Result<()> {
        self.engine.prune()?;
        let ids: Vec<_> = self.engine.groups.iter().map(|g| g.id).collect();
        for id in ids {
            let group = self.engine.get(id).unwrap();
            let active = group.active;
            let external = group
                .members
                .iter()
                .enumerate()
                .find(|(index, member)| {
                    *index != active
                        && unsafe { IsWindowVisible(platform::hwnd(&member.info.identity)) != 0 }
                })
                .map(|(index, _)| index);
            if let Some(index) = external {
                self.engine.activate(id, index, false)?;
            }
            let group = self.engine.get(id).unwrap();
            let window = platform::hwnd(&group.selected().unwrap().info.identity);
            if unsafe { IsIconic(window) } != 0 {
                continue;
            }
            if let Some(bounds) = platform::rect(window) {
                if unsafe { IsZoomed(window) } != 0 {
                    let work = platform::work_area(window);
                    let strip = platform::strip_height(window);
                    let bounds = work.fit(work, strip);
                    self.engine.set_bounds(id, bounds)?;
                    self.engine
                        .activate(id, self.engine.get(id).unwrap().active, false)?;
                } else {
                    let fitted =
                        bounds.fit(platform::work_area(window), platform::strip_height(window));
                    self.engine.set_bounds(id, fitted)?;
                    if fitted != bounds {
                        self.engine
                            .activate(id, self.engine.get(id).unwrap().active, false)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn snapshot(&self, status: String, quit: bool) -> Snapshot {
        let groups = self
            .engine
            .groups
            .iter()
            .map(|g| {
                let window = platform::hwnd(&g.selected().unwrap().info.identity);
                let shown = unsafe { IsIconic(window) == 0 && IsWindowVisible(window) != 0 };
                let mut members: Vec<_> = g.members.iter().map(|m| m.info.clone()).collect();
                for member in &mut members {
                    // Titles are refreshed without sending messages into the target process.
                    let mut title = [0u16; 1024];
                    let len = unsafe {
                        windows_sys::Win32::UI::WindowsAndMessaging::GetWindowTextW(
                            platform::hwnd(&member.identity),
                            title.as_mut_ptr(),
                            title.len() as i32,
                        )
                    };
                    if len > 0 {
                        member.title = String::from_utf16_lossy(&title[..len as usize]);
                    }
                }
                GroupView {
                    id: g.id,
                    name: g.name.clone(),
                    members,
                    active: g.active,
                    bounds: g.bounds,
                    strip_height: platform::strip_height(window),
                    shown,
                }
            })
            .collect();
        Snapshot {
            windows: self.candidates.clone(),
            groups,
            settings: self.settings.clone(),
            recovery: self.recovery.len(),
            status,
            ready: true,
            quit,
        }
    }
}

pub fn start(store: Store, window: isize, message: u32) -> Worker {
    let (sender, receiver) = mpsc::sync_channel(32);
    let mailbox = Arc::new(Mutex::new(None));
    let output = mailbox.clone();
    thread::Builder::new().name("window-management".into()).spawn(move || {
        let post = |snapshot| {
            if let Ok(mut output) = output.lock() { *output = Some(snapshot); }
            unsafe { PostMessageW(window as _, message, 0, 0); }
        };
        let mut controller = match Controller::open(store) {
            Ok(controller) => controller,
            Err(error) => { post(Snapshot { status:error, ..Default::default() }); return; },
        };
        controller.refresh();
        post(controller.snapshot("Select ordinary resizable windows, enter a group name, then choose Create group.".into(), false));
        let mut last_status = String::new();
        for command in receiver {
            let (status, quit) = match controller.action(command) { Ok(result) => result, Err(e) => (format!("Error: {e}"), false) };
            if !status.is_empty() { last_status = status; }
            post(controller.snapshot(last_status.clone(), quit));
            if quit { break; }
        }
    }).expect("Cannot start window worker");
    Worker { sender, mailbox }
}
