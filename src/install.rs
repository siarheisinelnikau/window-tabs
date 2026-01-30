//! Executable replacement transaction. Tests supply an in-memory filesystem and launcher.
use crate::{model::Result, update::InstallPlan};
use std::path::{Path, PathBuf};

pub trait InstallIo {
    fn hash(&mut self, path: &Path) -> Result<String>;
    /// Create a new file, never overwrite an existing path. Clean partial copies on failure.
    fn copy_new(&mut self, from: &Path, to: &Path) -> Result<()>;
    fn replace(&mut self, from: &Path, to: &Path) -> Result<()>;
    fn remove(&mut self, path: &Path) -> Result<()>;
    fn restart(&mut self, executable: &Path) -> Result<()>;
}

fn require_hash(io: &mut impl InstallIo, path: &Path, expected: &str) -> Result<()> {
    if io.hash(path)? != expected {
        return Err(format!("{} changed. Installation stopped.", path.display()));
    }
    Ok(())
}

/// The caller must validate the plan and wait for the original process to exit first.
/// No target write occurs until the payload and a new rollback copy have been verified.
pub fn replace_and_restart(plan: &InstallPlan, io: &mut impl InstallIo) -> Result<PathBuf> {
    let (replacement, backup) = plan.transaction_paths()?;
    let mut stage_owned = false;
    let result = (|| {
        require_hash(io, &plan.target, &plan.original_sha256)?;
        io.copy_new(&plan.staged, &replacement)?;
        stage_owned = true;
        require_hash(io, &replacement, &plan.sha256)?;
        io.copy_new(&plan.target, &backup)?;
        require_hash(io, &backup, &plan.original_sha256)?;
        require_hash(io, &plan.target, &plan.original_sha256)?;
        io.replace(&replacement, &plan.target)?;
        stage_owned = false;
        if let Err(launch) = io.restart(&plan.target) {
            let rollback: Result<()> = (|| {
                require_hash(io, &backup, &plan.original_sha256)?;
                io.copy_new(&backup, &replacement)?;
                stage_owned = true;
                require_hash(io, &replacement, &plan.original_sha256)?;
                io.replace(&replacement, &plan.target)?;
                stage_owned = false;
                Ok(())
            })();
            return match rollback {
                Ok(()) => {
                    let restarted = io.restart(&plan.target);
                    Err(format!(
                        "New version could not start: {launch}. The previous executable was restored. Previous-version restart: {}. Backup: {}",
                        restarted
                            .map(|_| "started".to_string())
                            .unwrap_or_else(|e| format!("failed: {e}")),
                        backup.display()
                    ))
                }
                Err(error) => Err(format!(
                    "New version could not start: {launch}. Rollback failed: {error}. Recovery copy: {}. Expected previous SHA-256: {}.",
                    backup.display(),
                    plan.original_sha256
                )),
            };
        }
        Ok(backup)
    })();
    if stage_owned {
        let _ = io.remove(&replacement);
    }
    result
}

#[cfg(windows)]
pub struct NativeInstallIo;

#[cfg(windows)]
impl InstallIo for NativeInstallIo {
    fn hash(&mut self, path: &Path) -> Result<String> {
        crate::update::hash_file(path)
    }
    fn copy_new(&mut self, from: &Path, to: &Path) -> Result<()> {
        use std::{
            fs::{self, File, OpenOptions},
            io::{self, Read},
        };
        let input = File::open(from).map_err(|e| format!("Cannot read {}: {e}", from.display()))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(|e| {
                format!(
                    "Cannot create {} without overwriting an existing file: {e}",
                    to.display()
                )
            })?;
        let result = (|| {
            let count = io::copy(&mut input.take(crate::update::MAX_EXE + 1), &mut output)
                .map_err(|e| e.to_string())?;
            if count > crate::update::MAX_EXE {
                return Err("Executable exceeds the size limit.".into());
            }
            output.sync_all().map_err(|e| e.to_string())
        })();
        drop(output);
        if result.is_err() {
            let _ = fs::remove_file(to);
        }
        result
    }
    fn replace(&mut self, from: &Path, to: &Path) -> Result<()> {
        crate::storage::replace_file(from, to)
    }
    fn remove(&mut self, path: &Path) -> Result<()> {
        std::fs::remove_file(path).map_err(|e| e.to_string())
    }
    fn restart(&mut self, executable: &Path) -> Result<()> {
        std::process::Command::new(executable)
            .current_dir(executable.parent().ok_or("Missing executable directory.")?)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
