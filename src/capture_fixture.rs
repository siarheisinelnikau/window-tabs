//! Non-sensitive fixtures for the real native controls. No live window or user data access.
use crate::{
    controller::{GroupView, Snapshot},
    model::{Identity, Rect, Selector, Template, WindowInfo},
    presentation::Page,
};
pub(crate) fn fixture(screen: &str) -> Result<(Page, Snapshot), String> {
    let page = match screen {
        "groups" | "empty" | "recovery" => Page::Groups,
        "picker" | "error" => Page::Picker(None),
        "saved" => Page::Saved,
        "settings" => Page::Settings,
        _ => return Err("Choose groups, empty, recovery, picker, saved, or settings.".into()),
    };
    let bounds = Rect {
        left: 80,
        top: 120,
        right: 980,
        bottom: 740,
    };
    let windows: Vec<_> = [
        ("Project notes — Editor", "Editor.exe"),
        ("Design research — Browser", "Browser.exe"),
        ("Assets — File Explorer", "explorer.exe"),
        ("Sprint planning — Calendar", "Calendar.exe"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (title, exe))| WindowInfo {
        identity: Identity {
            handle: 1000 + index as u64,
            pid: 9000 + index as u32,
            started: 123,
            class: "CaptureFixture".into(),
        },
        title: title.into(),
        executable: format!("C:\\Fixture\\{exe}"),
        bounds,
    })
    .collect();
    let empty = screen == "empty" || screen == "picker" || screen == "error";
    let mut snapshot=Snapshot{windows:windows.clone(),ready:true,status:"Choose a tab to switch windows. Your original window positions are kept for ungrouping.".into(),..Default::default()};
    if !empty {
        snapshot.groups = vec![
            GroupView {
                id: 1,
                name: "Design project".into(),
                members: windows[..3].to_vec(),
                active: 0,
                bounds,
                strip_height: 42,
                shown: true,
                restoring: false,
            },
            GroupView {
                id: 2,
                name: "Planning".into(),
                members: vec![windows[3].clone()],
                active: 0,
                bounds,
                strip_height: 42,
                shown: false,
                restoring: false,
            },
        ];
        snapshot.settings.templates = vec![Template {
            name: "Design project".into(),
            members: windows[..3]
                .iter()
                .map(|w| Selector {
                    executable: w.executable.clone(),
                    class: w.identity.class.clone(),
                    title: w.title.clone(),
                })
                .collect(),
        }];
    }
    if screen == "settings" {
        snapshot.status = "Startup checks are optional. Manual checking stays available.".into();
    }
    if screen == "saved" {
        snapshot.status =
            "Saved groups match open windows; they do not launch applications.".into();
    }
    if screen == "picker" {
        snapshot.status =
            "Ungrouping keeps your applications open and restores their original positions.".into();
    }
    if screen == "empty" {
        snapshot.status = "Create your first group to keep related windows together.".into();
    }
    if screen == "error" {
        snapshot.status="Error: A selected window is no longer available. Refresh windows, then choose the windows you want to group. Your applications have not been closed.".into();
    }
    if screen == "recovery" {
        snapshot.recovery = 2;
        snapshot.groups.clear();
    }
    Ok((page, snapshot))
}
