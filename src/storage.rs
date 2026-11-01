use crate::model::{Original, Result, Template};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

const SCHEMA: u32 = 1;
const MAX_JSON: u64 = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema: u32,
    pub check_on_startup: bool,
    pub language: crate::preferences::Language,
    pub theme: crate::preferences::Theme,
    pub templates: Vec<Template>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema: SCHEMA,
            check_on_startup: true,
            language: crate::preferences::Language::default(),
            theme: crate::preferences::Theme::default(),
            templates: vec![],
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    originals: Vec<Original>,
}

#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}

impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    pub fn settings(&self) -> Result<Settings> {
        let settings: Settings = read_json(&self.root.join("settings.json"))?.unwrap_or_default();
        if settings.schema != SCHEMA || settings.templates.len() > 64 {
            return Err("Unsupported or oversized settings file. Preserve it before starting a compatible version.".into());
        }
        Ok(settings)
    }
    pub fn save_settings(&self, settings: &Settings) -> Result<()> {
        atomic_json(&self.root.join("settings.json"), settings)
    }
    pub fn recovery(&self) -> Result<Vec<Original>> {
        let journal: Option<Journal> = read_json(&self.root.join("recovery.json"))?;
        match journal {
            Some(journal) if journal.schema == SCHEMA && journal.originals.len() <= 512 => Ok(journal.originals),
            Some(_) => Err("Unsupported recovery journal. Do not delete it until affected windows are restored.".into()),
            None => Ok(vec![]),
        }
    }
    pub fn save_recovery(&self, originals: &[Original]) -> Result<()> {
        atomic_json(
            &self.root.join("recovery.json"),
            &Journal {
                schema: SCHEMA,
                originals: originals.to_vec(),
            },
        )
    }
}

pub fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot open {}: {e}", path.display())),
    };
    let mut data = vec![];
    file.take(MAX_JSON + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_JSON {
        return Err(format!("{} exceeds the 8 MiB limit.", path.display()));
    }
    serde_json::from_slice(&data).map(Some).map_err(|e| {
        format!(
            "Cannot read {}: {e}. The file was preserved.",
            path.display()
        )
    })
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_JSON {
        return Err("Settings exceed the 8 MiB limit.".into());
    }
    let parent = path.parent().ok_or("Missing storage directory.")?;
    fs::create_dir_all(parent).map_err(|e| format!("Cannot create {}: {e}", parent.display()))?;
    let temporary = path.with_extension("json.tmp");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|_| file.sync_all())
        .map_err(|e| e.to_string())?;
    drop(file);
    replace_file(&temporary, path)
}

pub fn replace_file(from: &Path, to: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
        };
        let from: Vec<_> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<_> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: Both UTF-16 paths are NUL terminated and valid for this call.
        if unsafe {
            MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(format!(
                "Cannot replace stored file: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    fs::rename(from, to).map_err(|e| e.to_string())
}
