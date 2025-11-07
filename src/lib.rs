#[cfg(windows)]
mod controller;
pub mod model;
#[cfg(windows)]
pub mod platform;
pub mod storage;
#[cfg(windows)]
pub mod ui;
pub mod update;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPOSITORY: &str = "https://github.com/siarheisinelnikau/window-tabs";
