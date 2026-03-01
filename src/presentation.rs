//! Shared layout and update policy, independent of native handles.
use crate::model::GroupId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Page {
    #[default]
    Groups,
    Picker(Option<GroupId>),
    Saved,
    Settings,
}
pub const MIN_WIDTH: i32 = 820;
pub const MIN_HEIGHT: i32 = 600;
pub const ID_WINDOWS: i32 = 100;
pub const ID_NAME: i32 = 101;
pub const ID_GROUPS: i32 = 102;
pub const ID_MEMBERS: i32 = 103;
pub const ID_SAVED: i32 = 104;
pub const ID_REFRESH: i32 = 110;
pub const ID_CREATE: i32 = 111;
pub const ID_ADD: i32 = 112;
pub const ID_RENAME: i32 = 113;
pub const ID_SWITCH: i32 = 114;
pub const ID_LEFT: i32 = 115;
pub const ID_RIGHT: i32 = 116;
pub const ID_DETACH: i32 = 117;
pub const ID_CLOSE: i32 = 118;
pub const ID_SAVE: i32 = 119;
pub const ID_RESTORE: i32 = 120;
pub const ID_DELETE: i32 = 121;
pub const ID_RECOVER: i32 = 122;
pub const ID_STARTUP: i32 = 123;
pub const ID_CHECK: i32 = 124;
pub const ID_INSTALL: i32 = 125;
pub const ID_STATUS: i32 = 130;
pub const ID_UPDATE_STATUS: i32 = 131;
pub const ID_LABEL_WINDOWS: i32 = 140;
pub const ID_LABEL_GROUPS: i32 = 141;
pub const ID_LABEL_MEMBERS: i32 = 142;
pub const ID_LABEL_NAME: i32 = 143;
pub const ID_LABEL_SAVED: i32 = 144;
pub const ID_LABEL_SETTINGS: i32 = 145;
pub const ID_BRAND: i32 = 150;
pub const ID_TAGLINE: i32 = 151;
pub const ID_NAV_GROUPS: i32 = 152;
pub const ID_NAV_SAVED: i32 = 153;
pub const ID_NAV_SETTINGS: i32 = 154;
pub const ID_NEW: i32 = 155;
pub const ID_CANCEL: i32 = 156;
pub const ID_MORE: i32 = 157;
pub const ID_HELP: i32 = 158;
pub const ID_EMPTY: i32 = 159;
pub const ID_EMPTY_HELP: i32 = 160;
pub const ID_VERSION: i32 = 161;
pub const ID_PICK_COUNT: i32 = 162;

#[derive(Clone, Copy, Debug)]
pub struct Placement {
    pub id: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
pub fn layout(
    page: Page,
    width: i32,
    height: i32,
    has_groups: bool,
    has_saved: bool,
    recovery: bool,
) -> Vec<Placement> {
    let width = width.max(MIN_WIDTH);
    let height = height.max(MIN_HEIGHT);
    let mut result = vec![];
    let mut put = |id, x, y, width, height| {
        result.push(Placement {
            id,
            x,
            y,
            width,
            height,
        })
    };
    let m = 24;
    let full = width - 2 * m;
    let action = height - 98;
    put(ID_BRAND, m, 20, 320, 34);
    put(ID_TAGLINE, m, 58, full - 190, 24);
    put(ID_NAV_GROUPS, m, 94, 102, 34);
    put(ID_NAV_SAVED, m + 110, 94, 126, 34);
    put(ID_NAV_SETTINGS, m + 244, 94, 102, 34);
    if recovery || (!matches!(page, Page::Picker(_)) && (page != Page::Groups || has_groups)) {
        put(
            if recovery { ID_RECOVER } else { ID_NEW },
            width - m - 180,
            24,
            180,
            36,
        );
    }
    put(ID_STATUS, m, height - 55, full, 44);
    match page {
        Page::Groups => {
            let left = 220;
            let right = m + left + 24;
            let content = width - m - right;
            if has_groups {
                put(ID_LABEL_GROUPS, m, 150, left, 24);
                put(ID_GROUPS, m, 182, left, height - 294);
                put(ID_MORE, m, action, left, 34);
                put(ID_LABEL_MEMBERS, right, 150, content, 24);
                put(ID_NAME, right, 182, content - 106, 34);
                put(ID_RENAME, width - m - 98, 182, 98, 34);
                put(ID_MEMBERS, right, 230, content, height - 342);
                for (id, x, w) in [
                    (ID_SWITCH, 0, 112),
                    (ID_LEFT, 120, 88),
                    (ID_RIGHT, 216, 88),
                    (ID_DETACH, 312, 90),
                ] {
                    put(id, right + x, action, w, 34);
                }
            } else {
                put(ID_EMPTY, m + 24, 226, full - 48, 34);
                put(ID_EMPTY_HELP, m + 24, 272, full - 48, 84);
                if !recovery {
                    put(ID_NEW, m + 24, 370, 180, 36);
                }
            }
        }
        Page::Picker(add) => {
            put(ID_LABEL_WINDOWS, m, 150, full - 180, 26);
            put(ID_REFRESH, width - m - 160, 146, 160, 34);
            put(ID_HELP, m, 190, full, 30);
            let top = if add.is_none() {
                put(ID_LABEL_NAME, m, 236, 100, 26);
                put(ID_NAME, m + 110, 230, full - 110, 34);
                280
            } else {
                234
            };
            put(ID_WINDOWS, m, top, full, action - top - 16);
            put(ID_PICK_COUNT, m, action + 5, full - 300, 24);
            put(ID_CANCEL, width - m - 282, action, 104, 34);
            put(ID_CREATE, width - m - 168, action, 168, 34);
        }
        Page::Saved => {
            put(ID_LABEL_SAVED, m, 150, full, 26);
            put(ID_HELP, m, 190, full, 30);
            if has_saved {
                put(ID_SAVED, m, 234, full, height - 346);
            } else {
                put(ID_EMPTY, m + 24, 270, full - 48, 34);
                put(ID_EMPTY_HELP, m + 24, 314, full - 48, 84);
            }
            put(ID_RESTORE, m, action, 160, 34);
            put(ID_DELETE, m + 172, action, 130, 34);
        }
        Page::Settings => {
            put(ID_LABEL_SETTINGS, m, 150, full, 28);
            put(ID_HELP, m, 190, full, 38);
            put(ID_VERSION, m, 248, full, 32);
            put(ID_STARTUP, m, 300, full, 30);
            put(ID_CHECK, m, 354, 180, 36);
            put(ID_INSTALL, m + 192, 354, 220, 36);
            put(ID_UPDATE_STATUS, m, 416, full, action - 416 + 22);
        }
    }
    result
}

/// Native list controls retain scroll, focus and selection when their content is unchanged.
#[derive(Default)]
pub struct ContentCache(std::collections::HashMap<i32, Vec<String>>);
impl ContentCache {
    pub fn invalidate(&mut self, id: i32) {
        self.0.remove(&id);
    }
    pub fn changed(&mut self, id: i32, rows: Vec<String>) -> bool {
        if self.0.get(&id) == Some(&rows) {
            false
        } else {
            self.0.insert(id, rows);
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_screens_fit_at_minimum_and_large_sizes_without_overlapping_controls() {
        for (width, height) in [(MIN_WIDTH, MIN_HEIGHT), (960, 720)] {
            for page in [
                Page::Groups,
                Page::Picker(None),
                Page::Picker(Some(1)),
                Page::Saved,
                Page::Settings,
            ] {
                for populated in [true, false] {
                    let controls = layout(page, width, height, populated, populated, false);
                    for (i, a) in controls.iter().enumerate() {
                        assert!(
                            a.x >= 0
                                && a.y >= 0
                                && a.width > 0
                                && a.height > 0
                                && a.x + a.width <= width
                                && a.y + a.height <= height,
                            "{page:?} {a:?}"
                        );
                        for b in &controls[i + 1..] {
                            assert!(
                                a.x + a.width <= b.x
                                    || b.x + b.width <= a.x
                                    || a.y + a.height <= b.y
                                    || b.y + b.height <= a.y,
                                "{page:?}: {a:?} overlaps {b:?}"
                            );
                        }
                    }
                }
            }
        }
    }
    #[test]
    fn identical_snapshots_do_not_rebuild_lists() {
        let mut cache = ContentCache::default();
        assert!(cache.changed(ID_GROUPS, vec!["Work".into()]));
        for _ in 0..10 {
            assert!(!cache.changed(ID_GROUPS, vec!["Work".into()]));
        }
        assert!(cache.changed(ID_GROUPS, vec!["Work".into(), "Research".into()]));
    }
}
