//! Product configuration for the shared portable updater.
use crate::model::Result;
use std::{path::Path, sync::atomic::AtomicBool};

pub use rust_auto_updator::{Prepared, Release as Available, checksum, hash as hash_file};
pub const MAX_EXE: u64 = 128 * 1024 * 1024;

pub fn config(version: &str) -> Result<rust_auto_updator::Config> {
    let mut config =
        rust_auto_updator::Config::new("siarheisinelnikau", "window-tabs", version, "WindowTabs")?;
    config.max_executable_size = MAX_EXE;
    config.expected_subsystem = Some(2);
    config.install.stage_prefix = ".window-tabs-update-".into();
    config.install.helper_name = "WindowTabs-updater.exe".into();
    Ok(config)
}

pub fn select_release(json: &[u8], current: &str) -> Result<Option<Available>> {
    rust_auto_updator::parse_release(
        std::str::from_utf8(json).map_err(|e| e.to_string())?,
        &config(current)?,
    )
}

pub fn verify_pe(path: &Path) -> Result<()> {
    rust_auto_updator::verify_pe_subsystem(path, rust_auto_updator::Architecture::X64, Some(2))
}

pub fn check(cancel: &AtomicBool) -> Result<Option<Available>> {
    if cancel.load(std::sync::atomic::Ordering::Acquire) {
        return Err("Update cancelled.".into());
    }
    rust_auto_updator::check_latest(&config(crate::VERSION)?)
}

pub fn download(
    available: &Available,
    cancel: &AtomicBool,
    progress: impl FnMut(u64, u64),
) -> Result<Prepared> {
    rust_auto_updator::prepare_download_cancellable(
        available,
        &config(crate::VERSION)?,
        cancel,
        progress,
    )
}
