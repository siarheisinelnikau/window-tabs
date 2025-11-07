#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        if let Err(error) = window_tabs::ui::run() {
            window_tabs::ui::fatal(&error);
        }
    }
    #[cfg(not(windows))]
    eprintln!("WindowTabs requires Windows 10 version 1703 or later. Core tests are portable.");
}
