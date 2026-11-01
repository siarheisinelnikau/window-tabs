//! Original vector-style GDI icons and retained native owner-drawn controls.
//! No icon fonts, Shell extraction, downloaded art, GPU renderer or repaint timer.
use super::*;
use std::cell::Cell;

thread_local! {static DETAIL_FONT:Cell<HFONT>=const {Cell::new(null_mut())};}
pub(super) fn set_detail_font(font: HFONT) {
    DETAIL_FONT.with(|f| f.set(font));
}
pub(super) const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    r as u32 | (g as u32) << 8 | (b as u32) << 16
}

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Tabs,
    Bookmark,
    Settings,
    Plus,
    Back,
    More,
    Refresh,
    Pencil,
    Switch,
    Left,
    Right,
    Detach,
    Save,
    Delete,
    Recover,
    Download,
    Browser,
    Folder,
    Document,
    Terminal,
    Calendar,
}

pub(super) fn icon_for(id: i32) -> Icon {
    match id {
        ID_NAV_SAVED | ID_LABEL_SAVED => Icon::Bookmark,
        ID_NAV_SETTINGS | ID_LANGUAGE | ID_THEME => Icon::Settings,
        ID_LABEL_SETTINGS | ID_CHECK => Icon::Refresh,
        ID_NEW | ID_CREATE | ID_ADD => Icon::Plus,
        ID_CANCEL => Icon::Back,
        ID_MORE => Icon::More,
        ID_REFRESH => Icon::Refresh,
        ID_RENAME => Icon::Pencil,
        ID_SWITCH => Icon::Switch,
        ID_LEFT => Icon::Left,
        ID_RIGHT => Icon::Right,
        ID_DETACH => Icon::Detach,
        ID_SAVE => Icon::Save,
        ID_DELETE => Icon::Delete,
        ID_RECOVER | ID_RESTORE => Icon::Recover,
        ID_INSTALL => Icon::Download,
        _ => Icon::Tabs,
    }
}
pub(super) unsafe fn fill(dc: HDC, r: RECT, color: COLORREF) {
    unsafe {
        let b = CreateSolidBrush(color);
        FillRect(dc, &r, b);
        DeleteObject(b);
    }
}
pub(super) unsafe fn rounded(dc: HDC, r: RECT, color: COLORREF, border: COLORREF, radius: i32) {
    unsafe {
        let brush = CreateSolidBrush(color);
        let pen = CreatePen(PS_SOLID, 1, border);
        let old_b = SelectObject(dc, brush);
        let old_p = SelectObject(dc, pen);
        RoundRect(dc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(dc, old_b);
        SelectObject(dc, old_p);
        DeleteObject(brush);
        DeleteObject(pen);
    }
}
pub(super) unsafe fn icon(dc: HDC, glyph: Icon, x: i32, y: i32, size: i32, color: COLORREF) {
    unsafe {
        let saved = SaveDC(dc);
        let pen = CreatePen(PS_SOLID, (size / 12).max(1), color);
        SelectObject(dc, pen);
        SelectObject(dc, GetStockObject(NULL_BRUSH));
        let s = |v: i32| v * size / 24;
        let line = |a, b, c, d| {
            MoveToEx(dc, x + s(a), y + s(b), null_mut());
            LineTo(dc, x + s(c), y + s(d));
        };
        let rect = |a, b, c, d| {
            RoundRect(dc, x + s(a), y + s(b), x + s(c), y + s(d), s(3), s(3));
        };
        match glyph {
            Icon::Tabs => {
                rect(2, 3, 17, 16);
                rect(7, 8, 22, 21);
                line(8, 12, 21, 12);
                line(3, 7, 16, 7);
            }
            Icon::Bookmark => {
                line(6, 3, 18, 3);
                line(18, 3, 18, 22);
                line(18, 22, 12, 17);
                line(12, 17, 6, 22);
                line(6, 22, 6, 3);
            }
            Icon::Settings => {
                Ellipse(dc, x + s(5), y + s(5), x + s(19), y + s(19));
                Ellipse(dc, x + s(9), y + s(9), x + s(15), y + s(15));
                for (a, b, c, d) in [
                    (12, 1, 12, 5),
                    (12, 19, 12, 23),
                    (1, 12, 5, 12),
                    (19, 12, 23, 12),
                    (3, 3, 6, 6),
                    (18, 18, 21, 21),
                    (3, 21, 6, 18),
                    (18, 6, 21, 3),
                ] {
                    line(a, b, c, d);
                }
            }
            Icon::Plus => {
                line(12, 4, 12, 20);
                line(4, 12, 20, 12);
            }
            Icon::Back | Icon::Left => {
                line(20, 12, 4, 12);
                line(4, 12, 10, 6);
                line(4, 12, 10, 18);
            }
            Icon::Right => {
                line(4, 12, 20, 12);
                line(20, 12, 14, 6);
                line(20, 12, 14, 18);
            }
            Icon::More => {
                for a in [4, 11, 18] {
                    Ellipse(dc, x + s(a), y + s(10), x + s(a + 3), y + s(13));
                }
            }
            Icon::Refresh | Icon::Recover => {
                Arc(
                    dc,
                    x + s(3),
                    y + s(3),
                    x + s(21),
                    y + s(21),
                    x + s(19),
                    y + s(5),
                    x + s(20),
                    y + s(18),
                );
                line(19, 2, 19, 8);
                line(19, 8, 13, 8);
            }
            Icon::Pencil => {
                line(4, 17, 17, 4);
                line(17, 4, 21, 8);
                line(21, 8, 8, 21);
                line(8, 21, 3, 22);
                line(3, 22, 4, 17);
                line(14, 7, 18, 11);
            }
            Icon::Switch => {
                rect(3, 4, 21, 20);
                line(4, 8, 20, 8);
                line(7, 14, 17, 14);
                line(13, 10, 17, 14);
                line(17, 14, 13, 18);
            }
            Icon::Detach => {
                rect(3, 8, 16, 21);
                line(10, 3, 21, 3);
                line(21, 3, 21, 14);
                line(21, 3, 10, 14);
            }
            Icon::Save => {
                rect(3, 3, 21, 21);
                rect(7, 3, 17, 9);
                rect(7, 14, 17, 21);
            }
            Icon::Delete => {
                line(4, 6, 20, 6);
                line(9, 3, 15, 3);
                line(6, 7, 7, 21);
                line(7, 21, 17, 21);
                line(17, 21, 18, 7);
                line(10, 10, 10, 18);
                line(14, 10, 14, 18);
            }
            Icon::Download => {
                line(12, 2, 12, 16);
                line(6, 10, 12, 16);
                line(12, 16, 18, 10);
                line(3, 17, 3, 22);
                line(3, 22, 21, 22);
                line(21, 22, 21, 17);
            }
            Icon::Browser => {
                Ellipse(dc, x + s(3), y + s(3), x + s(21), y + s(21));
                Ellipse(dc, x + s(8), y + s(3), x + s(16), y + s(21));
                line(4, 9, 20, 9);
                line(4, 15, 20, 15);
            }
            Icon::Folder => {
                line(3, 8, 3, 4);
                line(3, 4, 10, 4);
                line(10, 4, 13, 8);
                rect(2, 8, 22, 20);
            }
            Icon::Document => {
                rect(5, 2, 20, 22);
                line(8, 7, 16, 7);
                line(8, 12, 16, 12);
                line(8, 17, 14, 17);
            }
            Icon::Terminal => {
                rect(2, 4, 22, 21);
                line(6, 9, 10, 13);
                line(10, 13, 6, 17);
                line(13, 17, 18, 17);
            }
            Icon::Calendar => {
                rect(3, 5, 21, 21);
                line(3, 10, 21, 10);
                line(8, 2, 8, 8);
                line(16, 2, 16, 8);
                line(8, 14, 11, 14);
                line(14, 14, 17, 14);
                line(8, 17, 11, 17);
            }
        }
        RestoreDC(dc, saved);
        DeleteObject(pen);
    }
}
fn caption(window: HWND) -> String {
    let mut text = [0u16; 2048];
    let n = unsafe { GetWindowTextW(window, text.as_mut_ptr(), text.len() as i32) };
    String::from_utf16_lossy(&text[..n.max(0) as usize])
}
unsafe fn text(dc: HDC, value: &str, mut bounds: RECT, color: COLORREF) {
    unsafe {
        SetTextColor(dc, color);
        SetBkMode(dc, TRANSPARENT as i32);
        DrawTextW(
            dc,
            wide(value).as_ptr(),
            -1,
            &mut bounds,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }
}
pub(super) unsafe fn draw(raw: *const DRAWITEMSTRUCT) -> LRESULT {
    if raw.is_null() {
        return 0;
    }
    unsafe {
        let item = &*raw;
        let saved = SaveDC(item.hDC);
        match item.CtlType {
            ODT_BUTTON => button(item),
            ODT_LISTBOX => row(item),
            ODT_STATIC => label(item),
            ODT_TAB => tab(item),
            _ => {}
        }
        RestoreDC(item.hDC, saved);
        1
    }
}
unsafe fn tab(item: &DRAWITEMSTRUCT) {
    unsafe {
        let theme = palette();
        let selected = item.itemState & ODS_SELECTED != 0;
        let mut title = [0u16; 512];
        let mut data = TCITEMW {
            mask: TCIF_TEXT,
            pszText: title.as_mut_ptr(),
            cchTextMax: title.len() as i32,
            ..std::mem::zeroed()
        };
        SendMessageW(
            item.hwndItem,
            TCM_GETITEMW,
            item.itemID as usize,
            &mut data as *mut _ as isize,
        );
        let len = title.iter().position(|c| *c == 0).unwrap_or(title.len());
        fill(item.hDC, item.rcItem, theme.background);
        rounded(
            item.hDC,
            item.rcItem,
            if selected {
                theme.selection
            } else {
                theme.surface
            },
            theme.border,
            8,
        );
        let mut rect = item.rcItem;
        rect.left += 10;
        rect.right -= 10;
        text(
            item.hDC,
            &String::from_utf16_lossy(&title[..len]),
            rect,
            theme.text,
        );
        if item.itemState & ODS_FOCUS != 0 {
            DrawFocusRect(item.hDC, &rect);
        }
    }
}
unsafe fn button(item: &DRAWITEMSTRUCT) {
    unsafe {
        let theme = palette();
        let dc = item.hDC;
        let id = item.CtlID as i32;
        let r = item.rcItem;
        let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
        let s = |v| v * dpi / 96;
        let nav = [ID_NAV_GROUPS, ID_NAV_SAVED, ID_NAV_SETTINGS].contains(&id);
        let active = nav && GetWindowLongPtrW(item.hwndItem, GWLP_USERDATA) != 0;
        let primary = [
            ID_NEW, ID_CREATE, ID_SWITCH, ID_RESTORE, ID_INSTALL, ID_RECOVER,
        ]
        .contains(&id);
        let disabled = item.itemState & ODS_DISABLED != 0;
        let down = item.itemState & ODS_SELECTED != 0;
        let bg = if disabled {
            theme.disabled
        } else if primary {
            if down {
                theme.accent_down
            } else {
                theme.accent
            }
        } else if active || down {
            theme.selection
        } else if nav {
            theme.background
        } else {
            theme.surface
        };
        let fg = if disabled {
            if theme.high_contrast {
                GetSysColor(COLOR_GRAYTEXT)
            } else {
                theme.secondary
            }
        } else if primary || (theme.high_contrast && (active || down)) {
            theme.on_accent
        } else if active {
            theme.accent
        } else {
            theme.text
        };
        fill(dc, r, theme.background);
        rounded(
            dc,
            r,
            bg,
            if nav || primary || disabled {
                bg
            } else {
                theme.border
            },
            s(10),
        );
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        if !font.is_null() {
            SelectObject(dc, font);
        }
        let glyph = icon_for(id);
        let size = s(17);
        let padding = s(10);
        let gap = s(8);
        icon(
            dc,
            glyph,
            r.left + padding,
            r.top + (r.bottom - r.top - size) / 2,
            size,
            fg,
        );
        text(
            dc,
            &caption(item.hwndItem),
            RECT {
                left: r.left + padding + size + gap,
                right: r.right - padding,
                ..r
            },
            fg,
        );
        if item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0 {
            let mut focus = r;
            InflateRect(&mut focus, -s(4), -s(4));
            DrawFocusRect(dc, &focus);
        }
    }
}
unsafe fn label(item: &DRAWITEMSTRUCT) {
    unsafe {
        let theme = palette();
        let r = item.rcItem;
        let dc = item.hDC;
        let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
        let s = |v| v * dpi / 96;
        fill(dc, r, theme.background);
        if item.CtlID as i32 == ID_EMPTY_ART {
            rounded(dc, r, theme.selection, theme.selection, s(24));
            icon(
                dc,
                Icon::Tabs,
                r.left + s(15),
                r.top + s(15),
                s(46),
                if theme.high_contrast {
                    theme.on_accent
                } else {
                    theme.accent
                },
            );
            return;
        }
        let size = s(20);
        icon(
            dc,
            icon_for(item.CtlID as i32),
            r.left,
            r.top + (r.bottom - r.top - size) / 2,
            size,
            theme.accent,
        );
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        if !font.is_null() {
            SelectObject(dc, font);
        }
        text(
            dc,
            &caption(item.hwndItem),
            RECT {
                left: r.left + s(30),
                ..r
            },
            theme.text,
        );
    }
}
unsafe fn row(item: &DRAWITEMSTRUCT) {
    unsafe {
        let theme = palette();
        let dc = item.hDC;
        let r = item.rcItem;
        fill(dc, r, theme.surface);
        if item.itemID == u32::MAX {
            return;
        }
        let n = SendMessageW(item.hwndItem, LB_GETTEXTLEN, item.itemID as usize, 0);
        if !(0..4096).contains(&n) {
            return;
        }
        let mut buffer = vec![0u16; n as usize + 1];
        SendMessageW(
            item.hwndItem,
            LB_GETTEXT,
            item.itemID as usize,
            buffer.as_mut_ptr() as isize,
        );
        let value = String::from_utf16_lossy(&buffer[..n as usize]);
        let (title, detail) = value.split_once('\n').unwrap_or((&value, ""));
        let dpi = GetDpiForWindow(item.hwndItem).max(96) as i32;
        let s = |v| v * dpi / 96;
        let mut inner = r;
        InflateRect(&mut inner, -s(4), -s(3));
        let selected = item.itemState & ODS_SELECTED != 0;
        let title_color = if theme.high_contrast && selected {
            theme.on_accent
        } else {
            theme.text
        };
        let detail_color = if theme.high_contrast && selected {
            theme.on_accent
        } else {
            theme.secondary
        };
        if selected {
            rounded(dc, inner, theme.selection, theme.selection, s(10));
        }
        let group = [ID_GROUPS, ID_SAVED].contains(&(item.CtlID as i32));
        let kind = if group {
            WindowKind::Application
        } else {
            window_kind(detail)
        };
        let (glyph, color, tint) = match kind {
            WindowKind::Browser => (Icon::Browser, rgb(46, 126, 125), rgb(225, 245, 241)),
            WindowKind::Files => (Icon::Folder, rgb(169, 114, 37), rgb(255, 244, 219)),
            WindowKind::Editor => (Icon::Document, rgb(75, 119, 193), rgb(230, 239, 255)),
            WindowKind::Terminal => (Icon::Terminal, rgb(135, 83, 177), rgb(242, 232, 255)),
            WindowKind::Calendar => (Icon::Calendar, rgb(189, 100, 70), rgb(255, 236, 226)),
            WindowKind::Application => {
                match title.bytes().fold(0u32, |a, b| a.wrapping_add(b as u32)) % 3 {
                    0 => (Icon::Tabs, theme.accent, theme.selection),
                    1 => (Icon::Tabs, rgb(46, 126, 125), rgb(225, 245, 241)),
                    _ => (Icon::Tabs, rgb(189, 100, 70), rgb(255, 236, 226)),
                }
            }
        };
        let color = if theme.high_contrast {
            theme.text
        } else {
            color
        };
        let tint = if theme.high_contrast {
            theme.surface
        } else {
            tint
        };
        let badge = RECT {
            left: inner.left + s(8),
            top: inner.top + s(7),
            right: inner.left + s(40),
            bottom: inner.top + s(39),
        };
        rounded(dc, badge, tint, tint, s(10));
        icon(dc, glyph, badge.left + s(6), badge.top + s(6), s(20), color);
        let font = SendMessageW(item.hwndItem, WM_GETFONT, 0, 0) as HFONT;
        if !font.is_null() {
            SelectObject(dc, font);
        }
        let left = badge.right + s(12);
        text(
            dc,
            title,
            RECT {
                left,
                top: inner.top + s(3),
                right: inner.right - s(10),
                bottom: inner.top + s(25),
            },
            title_color,
        );
        let small = DETAIL_FONT.with(|f| f.get());
        if !small.is_null() {
            SelectObject(dc, small);
        }
        text(
            dc,
            detail,
            RECT {
                left,
                top: inner.top + s(25),
                right: inner.right - s(10),
                bottom: inner.bottom - s(2),
            },
            detail_color,
        );
        if item.itemState & ODS_FOCUS != 0 && item.itemState & ODS_NOFOCUSRECT == 0 {
            let mut focus = inner;
            InflateRect(&mut focus, -1, -1);
            DrawFocusRect(dc, &focus);
        }
    }
}
pub(super) unsafe fn brand(dc: HDC, x: i32, y: i32, size: i32) {
    unsafe {
        let theme = palette();
        let r = RECT {
            left: x,
            top: y,
            right: x + size,
            bottom: y + size,
        };
        rounded(dc, r, theme.accent, theme.accent, size / 3);
        icon(
            dc,
            Icon::Tabs,
            x + size / 5,
            y + size / 5,
            size * 3 / 5,
            theme.on_accent,
        );
    }
}
