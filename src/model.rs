//! Window management state independent of Win32. Tests use synthetic adapters only.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub type Result<T> = std::result::Result<T, String>;
pub type GroupId = u64;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn translated(self, dx: i32, dy: i32) -> Self {
        Self {
            left: self.left.saturating_add(dx),
            top: self.top.saturating_add(dy),
            right: self.right.saturating_add(dx),
            bottom: self.bottom.saturating_add(dy),
        }
    }
    pub fn width(self) -> i32 {
        self.right.saturating_sub(self.left)
    }
    pub fn height(self) -> i32 {
        self.bottom.saturating_sub(self.top)
    }
    /// Keep the strip and usable content in the selected monitor's work area.
    pub fn fit(self, work: Rect, strip: i32) -> Self {
        let width = self.width().max(240).min(work.width().max(1));
        let height = self.height().max(120).min((work.height() - strip).max(1));
        let left = self
            .left
            .clamp(work.left, (work.right - width).max(work.left));
        let top = self.top.clamp(
            work.top + strip,
            (work.bottom - height).max(work.top + strip),
        );
        Self {
            left,
            top,
            right: left + width,
            bottom: top + height,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct Identity {
    pub handle: u64,
    pub pid: u32,
    pub started: u64,
    pub class: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowInfo {
    pub identity: Identity,
    pub title: String,
    pub executable: String,
    pub bounds: Rect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Placement {
    pub flags: u32,
    pub show: u32,
    pub min: [i32; 2],
    pub max: [i32; 2],
    pub normal: Rect,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Original {
    pub info: WindowInfo,
    pub placement: Placement,
    pub visible: bool,
    /// Physical coordinates let recovery detect a removed monitor without mixing spaces.
    #[serde(default)]
    pub normal_screen: Option<Rect>,
}

pub trait WindowAdapter {
    fn capture(&mut self, identity: &Identity) -> Result<Original>;
    fn alive(&self, identity: &Identity) -> bool;
    fn present(&mut self, identity: &Identity, bounds: Rect) -> Result<()>;
    fn hide(&mut self, identity: &Identity) -> Result<()>;
    fn restore(&mut self, original: &Original) -> Result<()>;
    fn focus(&mut self, identity: &Identity);
    /// Persist before the first side effect. Failure must prevent manipulation.
    fn checkpoint(&mut self, originals: &[Original]) -> Result<()>;
}

#[derive(Clone, Debug)]
pub struct Group {
    pub id: GroupId,
    pub name: String,
    pub members: Vec<Original>,
    pub active: usize,
    pub bounds: Rect,
    /// Once restoration starts, queued native requests must never race regrouping.
    pub restoring: bool,
}

impl Group {
    pub fn selected(&self) -> Option<&Original> {
        self.members.get(self.active)
    }
}

pub struct Groups<A: WindowAdapter> {
    pub adapter: A,
    pub groups: Vec<Group>,
    next_id: GroupId,
}

impl<A: WindowAdapter> Groups<A> {
    pub fn new(adapter: A) -> Self {
        Self {
            adapter,
            groups: vec![],
            next_id: 1,
        }
    }
    pub fn get(&self, id: GroupId) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }
    pub fn contains(&self, identity: &Identity) -> bool {
        self.groups
            .iter()
            .any(|g| g.members.iter().any(|m| &m.info.identity == identity))
    }
    pub fn checkpoint(&mut self) -> Result<()> {
        let entries: Vec<_> = self
            .groups
            .iter()
            .flat_map(|g| g.members.iter().cloned())
            .collect();
        self.adapter.checkpoint(&entries)
    }
    pub fn create(&mut self, name: &str, windows: &[Identity], bounds: Rect) -> Result<GroupId> {
        if bounds.width() < 1 || bounds.height() < 1 {
            return Err("Invalid window bounds.".into());
        }
        if windows.len() < 2 {
            return Err("Select at least two eligible windows.".into());
        }
        if self.groups.len() >= 16 {
            return Err("The limit is 16 active groups.".into());
        }
        let name = valid_name(name)?;
        let members = self.capture_unique(windows)?;
        let id = self.next_id;
        self.next_id += 1;
        self.groups.push(Group {
            id,
            name,
            members,
            active: 0,
            bounds,
            restoring: false,
        });
        if let Err(error) = self.checkpoint() {
            self.groups.pop();
            return Err(error);
        }
        if let Err(error) = self.activate(id, 0, true) {
            let recovery = self
                .close(id)
                .err()
                .map(|e| format!(" Recovery: {e}"))
                .unwrap_or_default();
            return Err(format!("Could not create group: {error}.{recovery}"));
        }
        Ok(id)
    }
    fn capture_unique(&mut self, windows: &[Identity]) -> Result<Vec<Original>> {
        if windows.len() > 32 {
            return Err("The limit is 32 windows per group.".into());
        }
        let mut seen = HashSet::new();
        let mut members = vec![];
        for identity in windows {
            if !seen.insert(identity.clone()) || self.contains(identity) {
                return Err("A window is duplicated or already belongs to a group.".into());
            }
            let original = self.adapter.capture(identity)?;
            if &original.info.identity != identity {
                return Err("Window identity changed while capturing its original placement. Refresh the list.".into());
            }
            members.push(original);
        }
        Ok(members)
    }
    pub fn add(&mut self, id: GroupId, windows: &[Identity]) -> Result<()> {
        self.require_active(id)?;
        if windows.is_empty() {
            return Err("Select at least one window to add.".into());
        }
        let index = self.index(id)?;
        if self.groups[index].members.len() + windows.len() > 32 {
            return Err("The limit is 32 windows per group.".into());
        }
        let new = self.capture_unique(windows)?;
        let old_len = self.groups[index].members.len();
        self.groups[index].members.extend(new);
        if let Err(error) = self.checkpoint() {
            self.groups[index].members.truncate(old_len);
            return Err(error);
        }
        let active = self.groups[index].active;
        // On error retain every original in the recovery journal and group.
        self.activate(id, active, true)
    }
    pub fn activate(&mut self, id: GroupId, member: usize, focus: bool) -> Result<()> {
        self.require_active(id)?;
        let index = self.index(id)?;
        let group = &self.groups[index];
        let identity = group
            .members
            .get(member)
            .ok_or("Unknown tab.")?
            .info
            .identity
            .clone();
        self.adapter.present(&identity, group.bounds)?;
        for (other, original) in group.members.iter().enumerate() {
            if other != member
                && self.adapter.alive(&original.info.identity)
                && let Err(error) = self.adapter.hide(&original.info.identity)
            {
                // The previous tab remains usable if hiding a member failed.
                let previous = &group.members[group.active].info.identity;
                let _ = self.adapter.present(previous, group.bounds);
                return Err(error);
            }
        }
        self.groups[index].active = member;
        if focus {
            self.adapter.focus(&identity);
        }
        Ok(())
    }
    pub fn rename(&mut self, id: GroupId, name: &str) -> Result<()> {
        let name = valid_name(name)?;
        let index = self.index(id)?;
        self.groups[index].name = name;
        Ok(())
    }
    pub fn reorder(&mut self, id: GroupId, from: usize, to: usize) -> Result<()> {
        self.require_active(id)?;
        let index = self.index(id)?;
        let group = &mut self.groups[index];
        if from >= group.members.len() || to >= group.members.len() {
            return Err("Unknown tab.".into());
        }
        let active = group.members[group.active].info.identity.clone();
        let member = group.members.remove(from);
        group.members.insert(to, member);
        group.active = group
            .members
            .iter()
            .position(|m| m.info.identity == active)
            .unwrap();
        Ok(())
    }
    pub fn detach(&mut self, id: GroupId, member: usize) -> Result<()> {
        let index = self.index(id)?;
        let group = &self.groups[index];
        let original = group.members.get(member).ok_or("Unknown tab.")?.clone();
        let was_active = group.active == member;
        if self.adapter.alive(&original.info.identity)
            && let Err(error) = self.adapter.restore(&original)
        {
            self.groups[index].restoring = true;
            return Err(format!(
                "Detachment did not finish: {error}. Close the group to retry restoration."
            ));
        }
        self.remove_member(index, member);
        self.checkpoint()?;
        if let Some(group) = self.get(id)
            && was_active
            && !group.restoring
        {
            self.activate(id, group.active, false)?;
        }
        Ok(())
    }
    pub fn close(&mut self, id: GroupId) -> Result<()> {
        let index = self.index(id)?;
        self.groups[index].restoring = true;
        let mut failed = vec![];
        let mut errors = vec![];
        for original in &self.groups[index].members {
            if self.adapter.alive(&original.info.identity)
                && let Err(error) = self.adapter.restore(original)
            {
                failed.push(original.clone());
                errors.push(format!("{}: {error}", original.info.title));
            }
        }
        if failed.is_empty() {
            self.groups.remove(index);
        } else {
            self.groups[index].members = failed;
            self.groups[index].active = 0;
        }
        self.checkpoint()?;
        if errors.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "Some windows could not be restored. Retry Close group. {}",
                errors.join("; ")
            ))
        }
    }
    pub fn close_all(&mut self) -> Result<()> {
        let ids: Vec<_> = self.groups.iter().map(|g| g.id).collect();
        let mut errors = vec![];
        for id in ids {
            if let Err(e) = self.close(id) {
                errors.push(e);
            }
        }
        // Retry a failed final journal write even if every window was restored.
        if let Err(error) = self.checkpoint() {
            errors.push(error);
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
    pub fn prune(&mut self) -> Result<()> {
        let mut changed = false;
        let mut remaining = self.groups.clone();
        let mut reveal = vec![];
        for group in &mut remaining {
            let active = group.selected().map(|m| m.info.identity.clone());
            let old_len = group.members.len();
            group
                .members
                .retain(|m| self.adapter.alive(&m.info.identity));
            changed |= old_len != group.members.len();
            group.active = active
                .as_ref()
                .and_then(|a| group.members.iter().position(|m| &m.info.identity == a))
                .unwrap_or(0);
            if !group.restoring
                && !group.members.is_empty()
                && active.is_some_and(|id| !group.members.iter().any(|m| m.info.identity == id))
            {
                reveal.push((group.id, group.active));
            }
        }
        remaining.retain(|g| !g.members.is_empty());
        if changed {
            let entries: Vec<_> = remaining
                .iter()
                .flat_map(|g| g.members.iter().cloned())
                .collect();
            self.adapter.checkpoint(&entries)?;
            self.groups = remaining;
            // Removing an inactive member must not restore a minimized group.
            // Changes in one group must not manipulate another group at all.
            for (id, active) in reveal {
                self.activate(id, active, false)?;
            }
        }
        Ok(())
    }
    pub fn set_bounds(&mut self, id: GroupId, bounds: Rect) -> Result<()> {
        self.require_active(id)?;
        if bounds.width() < 1 || bounds.height() < 1 {
            return Err("Invalid window bounds.".into());
        }
        let index = self.index(id)?;
        self.groups[index].bounds = bounds;
        Ok(())
    }
    fn remove_member(&mut self, index: usize, member: usize) {
        let group = &mut self.groups[index];
        group.members.remove(member);
        if member < group.active {
            group.active -= 1;
        }
        group.active = group.active.min(group.members.len().saturating_sub(1));
        if group.members.is_empty() {
            self.groups.remove(index);
        }
    }
    fn index(&self, id: GroupId) -> Result<usize> {
        self.groups
            .iter()
            .position(|g| g.id == id)
            .ok_or("Unknown group.".into())
    }
    fn require_active(&self, id: GroupId) -> Result<()> {
        let group = self.get(id).ok_or("Unknown group.")?;
        if group.restoring {
            Err("This group is awaiting restoration. Retry Close group or Detach; switching and regrouping are disabled.".into())
        } else {
            Ok(())
        }
    }
}

pub fn valid_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        Err("Use a group name of 1 to 80 characters without control characters.".into())
    } else {
        Ok(name.to_owned())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Selector {
    pub executable: String,
    pub class: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    pub members: Vec<Selector>,
}

impl Template {
    pub fn from_group(group: &Group) -> Self {
        Self {
            name: group.name.clone(),
            members: group
                .members
                .iter()
                .map(|m| Selector {
                    executable: m.info.executable.clone(),
                    class: m.info.identity.class.clone(),
                    title: m.info.title.clone(),
                })
                .collect(),
        }
    }
    /// Never launch applications or guess between multiple matching windows.
    pub fn resolve(&self, candidates: &[WindowInfo]) -> Result<Vec<Identity>> {
        if !(2..=32).contains(&self.members.len()) {
            return Err("Saved group must contain 2 to 32 windows.".into());
        }
        let mut used = HashSet::new();
        let mut result = vec![];
        for selector in &self.members {
            let matches: Vec<_> = candidates
                .iter()
                .filter(|w| {
                    w.executable.eq_ignore_ascii_case(&selector.executable)
                        && w.identity.class == selector.class
                        && w.title == selector.title
                        && !used.contains(&w.identity)
                })
                .collect();
            if matches.len() != 1 {
                return Err(format!(
                    "Saved window '{}' is missing or ambiguous. Open it with the same title, refresh, or create a new group.",
                    selector.title
                ));
            }
            let identity = matches[0].identity.clone();
            used.insert(identity.clone());
            result.push(identity);
        }
        Ok(result)
    }
}
