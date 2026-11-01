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
pub const ID_EMPTY_ART: i32 = 163;
pub const ID_LANGUAGE: i32 = 164;
pub const ID_THEME: i32 = 165;
pub const ID_LABEL_LANGUAGE: i32 = 166;
pub const ID_LABEL_THEME: i32 = 167;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Browser,
    Files,
    Editor,
    Terminal,
    Calendar,
    Application,
}
pub fn window_kind(executable: &str) -> WindowKind {
    let name = executable.to_ascii_lowercase();
    if ["chrome", "firefox", "msedge", "brave", "browser"]
        .iter()
        .any(|s| name.contains(s))
    {
        WindowKind::Browser
    } else if name.contains("explorer") {
        WindowKind::Files
    } else if ["terminal", "powershell", "cmd", "console"]
        .iter()
        .any(|s| name.contains(s))
    {
        WindowKind::Terminal
    } else if name.contains("calendar") {
        WindowKind::Calendar
    } else if ["editor", "notepad", "code", "sublime"]
        .iter()
        .any(|s| name.contains(s))
    {
        WindowKind::Editor
    } else {
        WindowKind::Application
    }
}
pub fn clean_row_title(title: &str) -> String {
    title
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}
pub fn window_row(
    window: &crate::model::WindowInfo,
    active: bool,
    language: crate::preferences::Language,
) -> String {
    let executable = window
        .executable
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or("Application");
    let app = executable
        .rsplit_once('.')
        .map(|(name, _)| name)
        .unwrap_or(executable);
    format!(
        "{}\n{}{}",
        clean_row_title(&window.title),
        clean_row_title(app),
        if active {
            format!(
                " · {}",
                crate::localization::text(language, "Active window")
            )
        } else {
            String::new()
        }
    )
}
pub fn group_row(
    name: &str,
    count: usize,
    state: &str,
    language: crate::preferences::Language,
) -> String {
    let noun = if language == crate::preferences::Language::Russian {
        match (count % 100, count % 10) {
            (11..=14, _) => "окон".to_owned(),
            (_, 1) => "окно".to_owned(),
            (_, 2..=4) => "окна".to_owned(),
            _ => "окон".to_owned(),
        }
    } else {
        crate::localization::text(language, if count == 1 { "window" } else { "windows" })
    };
    format!(
        "{}\n{} {}{}",
        clean_row_title(name),
        count,
        noun,
        if state.is_empty() {
            String::new()
        } else {
            format!(" · {}", crate::localization::text(language, state))
        }
    )
}

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
    put(ID_BRAND, m + 56, 20, 320, 34);
    put(ID_TAGLINE, m + 56, 58, full - 306, 24);
    put(ID_NAV_GROUPS, m, 94, 124, 38);
    put(ID_NAV_SAVED, m + 132, 94, 238, 38);
    put(ID_NAV_SETTINGS, m + 378, 94, 176, 38);
    if recovery || (!matches!(page, Page::Picker(_)) && (page != Page::Groups || has_groups)) {
        put(
            if recovery { ID_RECOVER } else { ID_NEW },
            width - m - 244,
            24,
            244,
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
                put(ID_NAME, right, 182, content - 184, 34);
                put(ID_RENAME, width - m - 176, 182, 176, 34);
                put(ID_MEMBERS, right, 230, content, height - 342);
                for (id, x, w) in [
                    (ID_SWITCH, 0, 136),
                    (ID_LEFT, 144, 112),
                    (ID_RIGHT, 264, 112),
                    (ID_DETACH, 384, 144),
                ] {
                    put(id, right + x, action, w, 34);
                }
            } else {
                put(ID_EMPTY_ART, m + 24, 208, 76, 76);
                put(ID_EMPTY, m + 122, 216, full - 146, 34);
                put(ID_EMPTY_HELP, m + 122, 266, full - 146, 84);
                if !recovery {
                    put(ID_NEW, m + 122, 370, 180, 36);
                }
            }
        }
        Page::Picker(add) => {
            put(ID_LABEL_WINDOWS, m, 150, full - 232, 26);
            put(ID_REFRESH, width - m - 216, 146, 216, 34);
            put(ID_HELP, m, 190, full, 30);
            let top = if add.is_none() {
                put(ID_LABEL_NAME, m, 236, 132, 26);
                put(ID_NAME, m + 142, 230, full - 142, 34);
                280
            } else {
                234
            };
            put(ID_WINDOWS, m, top, full, action - top - 16);
            put(ID_PICK_COUNT, m, action + 5, full - 316, 24);
            put(ID_CANCEL, width - m - 298, action, 104, 34);
            put(ID_CREATE, width - m - 184, action, 184, 34);
        }
        Page::Saved => {
            put(ID_LABEL_SAVED, m, 150, full, 26);
            put(ID_HELP, m, 190, full, 30);
            if has_saved {
                put(ID_SAVED, m, 234, full, height - 346);
            } else {
                put(ID_EMPTY_ART, m + 24, 256, 76, 76);
                put(ID_EMPTY, m + 122, 266, full - 146, 34);
                put(ID_EMPTY_HELP, m + 122, 310, full - 146, 84);
            }
            put(ID_RESTORE, m, action, 216, 34);
            put(ID_DELETE, m + 228, action, 216, 34);
        }
        Page::Settings => {
            put(ID_LABEL_SETTINGS, m, 150, full, 28);
            put(ID_LABEL_LANGUAGE, m, 198, 120, 26);
            put(ID_LANGUAGE, m + 128, 190, 204, 36);
            put(ID_LABEL_THEME, m + 364, 198, 120, 26);
            put(ID_THEME, m + 492, 190, 204, 36);
            put(ID_HELP, m, 246, full, 38);
            put(ID_VERSION, m, 298, full, 32);
            put(ID_STARTUP, m, 346, full, 30);
            put(ID_CHECK, m, 394, 244, 36);
            put(ID_INSTALL, m + 256, 394, 320, 36);
            put(ID_UPDATE_STATUS, m, 448, full, action - 448 + 22);
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
                    for recovery in [false, true] {
                        let controls = layout(page, width, height, populated, populated, recovery);
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
    #[test]
    fn translated_metadata_keeps_group_names_and_correct_russian_counts() {
        use crate::preferences::Language;
        assert_eq!(
            group_row("Saved", 2, "Saved", Language::French),
            "Saved\n2 fenêtres · Enregistré"
        );
        for (count, noun) in [
            (1, "окно"),
            (2, "окна"),
            (5, "окон"),
            (11, "окон"),
            (21, "окно"),
            (24, "окна"),
        ] {
            assert_eq!(
                group_row("Settings", count, "", Language::Russian),
                format!("Settings\n{count} {noun}")
            );
        }
    }
    #[cfg(windows)]
    #[test]
    fn translated_action_captions_fit_native_font_at_minimum_size() {
        use windows_sys::{
            Win32::{Foundation::SIZE, Graphics::Gdi::*},
            core::w,
        };
        let captions = [
            (ID_NAV_GROUPS, "Groups"),
            (ID_NAV_SAVED, "Saved groups"),
            (ID_NAV_SETTINGS, "Settings"),
            (ID_NEW, "New group"),
            (ID_RECOVER, "Recover windows"),
            (ID_MORE, "Group actions…"),
            (ID_RENAME, "Rename"),
            (ID_SWITCH, "Switch"),
            (ID_LEFT, "Left"),
            (ID_RIGHT, "Right"),
            (ID_DETACH, "Detach"),
            (ID_REFRESH, "Refresh windows"),
            (ID_CANCEL, "Back"),
            (ID_CREATE, "Create group"),
            (ID_CREATE, "Add windows"),
            (ID_RESTORE, "Restore saved"),
            (ID_DELETE, "Delete saved"),
            (ID_CHECK, "Check for updates"),
            (ID_INSTALL, "Download and install"),
        ];
        // A memory DC measures the actual native font without opening an app or discovering windows.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            assert!(!dc.is_null());
            let font = CreateFontW(
                -15,
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
            assert!(!font.is_null());
            let previous = SelectObject(dc, font);
            let mut failures = vec![];
            for language in crate::preferences::Language::ALL {
                for page in [
                    Page::Groups,
                    Page::Picker(None),
                    Page::Saved,
                    Page::Settings,
                ] {
                    for recovery in [false, true] {
                        for control in layout(page, MIN_WIDTH, MIN_HEIGHT, true, true, recovery) {
                            for (_, source) in captions.iter().filter(|(id, _)| *id == control.id) {
                                let translated = crate::localization::text(language, source);
                                let text: Vec<u16> = translated.encode_utf16().collect();
                                let mut size = SIZE::default();
                                assert_ne!(
                                    GetTextExtentPoint32W(
                                        dc,
                                        text.as_ptr(),
                                        text.len() as i32,
                                        &mut size
                                    ),
                                    0
                                );
                                if size.cx > control.width - 45 {
                                    failures.push(format!(
                                        "{language:?}: {source}: {} > {}",
                                        size.cx,
                                        control.width - 45
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            SelectObject(dc, previous);
            DeleteObject(font);
            DeleteDC(dc);
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }
    #[test]
    fn display_rows_keep_metadata_separate_without_changing_window_titles() {
        use crate::model::{Identity, Rect, WindowInfo};
        let mut window = WindowInfo {
            identity: Identity {
                handle: 1,
                pid: 2,
                started: 3,
                class: "Editor".into(),
            },
            title: "Notes\nBudget\t2026\0.txt".into(),
            executable: r"C:\Program Files\Editor\Editor.exe".into(),
            bounds: Rect::default(),
        };
        assert_eq!(
            window_row(&window, true, crate::preferences::Language::English),
            "Notes Budget 2026 .txt\nEditor · Active window"
        );
        assert!(window.title.contains('\n'));
        window.executable = "/apps/notes/editor".into();
        assert_eq!(
            window_row(&window, false, crate::preferences::Language::English)
                .split_once('\n')
                .unwrap()
                .1,
            "editor"
        );
        assert_eq!(
            group_row(
                "Work\nSaved",
                1,
                "Minimized",
                crate::preferences::Language::English
            ),
            "Work Saved\n1 window · Minimized"
        );
        assert_eq!(
            group_row("Work", 2, "", crate::preferences::Language::English),
            "Work\n2 windows"
        );
    }
    #[test]
    fn window_icons_use_application_names_and_have_a_generic_fallback() {
        for (name, kind) in [
            ("Chrome", WindowKind::Browser),
            ("MSedge", WindowKind::Browser),
            ("explorer", WindowKind::Files),
            ("powershell", WindowKind::Terminal),
            ("Calendar", WindowKind::Calendar),
            ("notepad", WindowKind::Editor),
            ("My application", WindowKind::Application),
        ] {
            assert_eq!(window_kind(name), kind, "{name}");
        }
    }
}
