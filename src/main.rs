#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    {
        if std::env::args_os()
            .nth(1)
            .is_some_and(|arg| arg == "--capture")
        {
            if let Err(error) = window_tabs::ui::capture() {
                eprintln!("Capture failed: {error}");
                std::process::exit(1);
            }
            return;
        }
        if let Err(error) = window_tabs::ui::run() {
            window_tabs::ui::fatal(&error);
        }
    }
    #[cfg(not(windows))]
    eprintln!(
        "WindowTabs requires Windows 10 version 1703 or later. The shared updater currently targets Windows."
    );
}
