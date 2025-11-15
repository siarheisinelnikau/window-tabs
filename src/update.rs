//! Stable-channel release selection and a portable, user-initiated updater.
use crate::model::Result;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const MAX_EXE: u64 = 128 * 1024 * 1024;
const PREFIX: &str = "https://github.com/siarheisinelnikau/window-tabs/releases/download/";

#[derive(Clone, Debug, Deserialize)]
pub struct Asset {
    pub name: String,
    pub browser_download_url: String,
    pub size: u64,
}
#[derive(Debug, Deserialize)]
pub struct Release {
    pub tag_name: String,
    pub draft: bool,
    pub prerelease: bool,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug)]
pub struct Available {
    pub version: Version,
    pub executable: Asset,
    pub checksums: Asset,
}

pub fn select_release(json: &[u8], current: &str) -> Result<Option<Available>> {
    let release: Release =
        serde_json::from_slice(json).map_err(|e| format!("Invalid release response: {e}"))?;
    let version = Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|e| format!("Invalid release version: {e}"))?;
    let installed = Version::parse(current).map_err(|e| e.to_string())?;
    if release.draft
        || release.prerelease
        || !version.pre.is_empty()
        || version.cmp_precedence(&installed).is_le()
    {
        return Ok(None);
    }
    let name = format!("WindowTabs-v{version}.exe");
    let executable = release
        .assets
        .iter()
        .find(|a| a.name == name)
        .ok_or("New release has no compatible standalone executable.")?
        .clone();
    let checksums = release
        .assets
        .iter()
        .find(|a| a.name == "SHA256SUMS.txt")
        .ok_or("The release has no SHA256SUMS.txt; installation is unavailable.")?
        .clone();
    for asset in [&executable, &checksums] {
        let expected = format!("{PREFIX}{}/{}", release.tag_name, asset.name);
        if asset.browser_download_url != expected || asset.size == 0 || asset.size > MAX_EXE {
            return Err("Release asset URL or size is invalid.".into());
        }
    }
    if checksums.size > 64 * 1024 {
        return Err("Checksum manifest is too large.".into());
    }
    Ok(Some(Available {
        version,
        executable,
        checksums,
    }))
}

pub fn checksum(manifest: &str, filename: &str) -> Result<String> {
    let mut found = None;
    for line in manifest.lines() {
        let mut parts = line.split_whitespace();
        let (Some(hash), Some(name)) = (parts.next(), parts.next()) else {
            continue;
        };
        if name.trim_start_matches('*') != filename {
            continue;
        }
        if parts.next().is_some()
            || hash.len() != 64
            || !hash.bytes().all(|b| b.is_ascii_hexdigit())
            || found.is_some()
        {
            return Err("Checksum manifest contains an invalid or duplicate entry.".into());
        }
        found = Some(hash.to_ascii_lowercase());
    }
    found.ok_or("The executable is missing from the checksum manifest.".into())
}

pub fn hash_file(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hash = Sha256::new();
    let mut bytes = [0u8; 64 * 1024];
    let mut total = 0;
    loop {
        let count = file.read(&mut bytes).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        total += count as u64;
        if total > MAX_EXE {
            return Err("Update exceeds the size limit.".into());
        }
        hash.update(&bytes[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn verify_pe(path: &Path) -> Result<()> {
    use std::io::{Seek, SeekFrom};
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut header = [0u8; 64];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..2] != b"MZ" {
        return Err("Downloaded asset is not a Windows executable.".into());
    }
    let offset = u32::from_le_bytes(header[60..64].try_into().unwrap()) as u64;
    if offset > 1024 * 1024 {
        return Err("Invalid PE header offset.".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 6];
    file.read_exact(&mut pe).map_err(|e| e.to_string())?;
    if &pe[..4] != b"PE\0\0" || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664 {
        return Err("The update is not a compatible x64 executable.".into());
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InstallPlan {
    pub parent_pid: u32,
    pub target: PathBuf,
    pub staged: PathBuf,
    pub sha256: String,
}

impl InstallPlan {
    pub fn validate(&self, helper: &Path) -> Result<()> {
        if !self.target.is_absolute()
            || !self.staged.is_absolute()
            || self.target == self.staged
            || self.target == helper
            || self
                .target
                .extension()
                .is_none_or(|ext| !ext.eq_ignore_ascii_case("exe"))
            || self.staged.parent() != helper.parent()
            || self.parent_pid == 0
            || self.sha256.len() != 64
            || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("Invalid update installation plan.".into());
        }
        if hash_file(&self.staged)? != self.sha256 {
            return Err("Update checksum mismatch; the installed executable was preserved.".into());
        }
        verify_pe(&self.staged)
    }
}

#[cfg(windows)]
pub mod native {
    use super::*;
    use crate::{platform::wide, storage};
    use std::{
        ffi::c_void,
        ptr::{null, null_mut},
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{Foundation::*, Networking::WinHttp::*, System::Threading::*};

    struct Http(*mut c_void);
    impl Drop for Http {
        fn drop(&mut self) {
            unsafe {
                WinHttpCloseHandle(self.0);
            }
        }
    }
    fn err(action: &str) -> String {
        format!("{action}: {}", std::io::Error::last_os_error())
    }

    fn get(
        url: &str,
        limit: u64,
        output: &mut impl Write,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64),
    ) -> Result<u64> {
        let tail = url
            .strip_prefix("https://")
            .ok_or("Only HTTPS downloads are allowed.")?;
        let (host, path) = tail.split_once('/').ok_or("Invalid update URL.")?;
        if !["api.github.com", "github.com"].contains(&host) {
            return Err("Update URL has an untrusted origin.".into());
        }
        unsafe {
            let agent = wide(concat!("WindowTabs/", env!("CARGO_PKG_VERSION")));
            let session = Http(WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            ));
            if session.0.is_null() {
                return Err(err("Cannot open HTTPS session"));
            }
            WinHttpSetTimeouts(session.0, 5000, 10000, 10000, 15000);
            let host = wide(host);
            let connection = Http(WinHttpConnect(session.0, host.as_ptr(), 443, 0));
            if connection.0.is_null() {
                return Err(err("Cannot connect to GitHub"));
            }
            let verb = wide("GET");
            let path = wide(&format!("/{path}"));
            let request = Http(WinHttpOpenRequest(
                connection.0,
                verb.as_ptr(),
                path.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            ));
            if request.0.is_null() {
                return Err(err("Cannot open update request"));
            }
            let policy: u32 = WINHTTP_OPTION_REDIRECT_POLICY_DISALLOW_HTTPS_TO_HTTP;
            if WinHttpSetOption(
                request.0,
                WINHTTP_OPTION_REDIRECT_POLICY,
                &policy as *const _ as *const c_void,
                4,
            ) == 0
            {
                return Err(err("Cannot secure redirects"));
            }
            let headers =
                wide("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28\r\n");
            if cancel.load(Ordering::Relaxed) {
                return Err("Update cancelled.".into());
            }
            if WinHttpSendRequest(
                request.0,
                headers.as_ptr(),
                (headers.len() - 1) as u32,
                null(),
                0,
                0,
                0,
            ) == 0
                || WinHttpReceiveResponse(request.0, null_mut()) == 0
            {
                return Err(err("HTTPS request failed"));
            }
            let mut status = 0u32;
            let mut length = 4;
            if WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                &mut status as *mut _ as *mut c_void,
                &mut length,
                null_mut(),
            ) == 0
            {
                return Err(err("Cannot read HTTP status"));
            }
            match status {
                200 => {}
                404 => return Err(
                    "The repository or a published stable release is not available yet (HTTP 404)."
                        .into(),
                ),
                403 | 429 => {
                    return Err(format!(
                        "GitHub refused the request or its rate limit was reached (HTTP {status}). Try again later."
                    ));
                }
                _ => return Err(format!("Update server returned HTTP {status}.")),
            }
            let deadline = Instant::now() + Duration::from_secs(180);
            let mut total = 0;
            let mut buffer = [0u8; 65536];
            loop {
                if cancel.load(Ordering::Relaxed) || Instant::now() > deadline {
                    return Err("Update cancelled or download timed out.".into());
                }
                let mut count = 0;
                if WinHttpReadData(
                    request.0,
                    buffer.as_mut_ptr() as *mut c_void,
                    buffer.len() as u32,
                    &mut count,
                ) == 0
                {
                    return Err(err("Download failed"));
                }
                if count == 0 {
                    break;
                }
                total += count as u64;
                if total > limit {
                    return Err("Response exceeds the permitted size.".into());
                }
                output
                    .write_all(&buffer[..count as usize])
                    .map_err(|e| format!("Cannot write update: {e}"))?;
                progress(total);
            }
            Ok(total)
        }
    }
    pub fn check(cancel: &AtomicBool) -> Result<Option<Available>> {
        if !cfg!(target_arch = "x86_64") {
            return Err("Automatic installation is supported only for the x64 package.".into());
        }
        let mut bytes = vec![];
        get(
            "https://api.github.com/repos/siarheisinelnikau/window-tabs/releases/latest",
            2 * 1024 * 1024,
            &mut bytes,
            cancel,
            |_| {},
        )?;
        select_release(&bytes, crate::VERSION)
    }
    pub fn download(
        available: &Available,
        root: &Path,
        cancel: Arc<AtomicBool>,
        progress: impl FnMut(u64),
    ) -> Result<PathBuf> {
        let directory =
            root.join("updates")
                .join(format!("{}-{}", available.version, std::process::id()));
        fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let directory = fs::canonicalize(directory).map_err(|e| e.to_string())?;
        let mut manifest = vec![];
        get(
            &available.checksums.browser_download_url,
            64 * 1024,
            &mut manifest,
            &cancel,
            |_| {},
        )?;
        let expected = checksum(
            std::str::from_utf8(&manifest).map_err(|e| e.to_string())?,
            &available.executable.name,
        )?;
        let staged = directory.join("payload.exe");
        let mut file = fs::File::create(&staged).map_err(|e| e.to_string())?;
        let count = get(
            &available.executable.browser_download_url,
            available.executable.size,
            &mut file,
            &cancel,
            progress,
        )?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if count != available.executable.size || hash_file(&staged)? != expected {
            return Err(
                "Update verification failed. The installed executable was preserved.".into(),
            );
        }
        verify_pe(&staged)?;
        let target = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(|e| e.to_string())?;
        let helper = directory.join("WindowTabs-updater.exe");
        fs::copy(&target, &helper).map_err(|e| format!("Cannot prepare update helper: {e}"))?;
        let plan = InstallPlan {
            parent_pid: std::process::id(),
            target,
            staged,
            sha256: expected,
        };
        let path = directory.join("install.json");
        storage::atomic_json(&path, &plan)?;
        Ok(path)
    }
    pub fn launch_helper(plan: &Path) -> Result<()> {
        use std::os::windows::process::CommandExt;
        let helper = plan
            .parent()
            .ok_or("Missing update directory.")?
            .join("WindowTabs-updater.exe");
        std::process::Command::new(helper)
            .arg("--apply-update")
            .arg(plan)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("Cannot start update helper: {e}"))?;
        Ok(())
    }
    /// Called only in a separate helper process after the user requests installation.
    pub fn apply(plan_path: &Path) -> Result<()> {
        let helper = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(|e| e.to_string())?;
        let plan_path = fs::canonicalize(plan_path).map_err(|e| e.to_string())?;
        if plan_path.parent() != helper.parent() {
            return Err("Update plan is outside the helper directory.".into());
        }
        let plan: InstallPlan =
            storage::read_json(&plan_path)?.ok_or("Missing installation plan.")?;
        plan.validate(&helper)?;
        unsafe {
            let parent = OpenProcess(PROCESS_SYNCHRONIZE, 0, plan.parent_pid);
            if !parent.is_null() {
                let result = WaitForSingleObject(parent, 30000);
                CloseHandle(parent);
                if result != WAIT_OBJECT_0 {
                    return Err("WindowTabs did not close. The update was not installed.".into());
                }
            } else if GetLastError() != ERROR_INVALID_PARAMETER {
                return Err(err("Cannot wait for the running application"));
            }
        }
        // Stage on the destination volume, then replace in one filesystem operation.
        let replacement = plan.target.with_extension("update.tmp");
        let backup = plan.target.with_extension("previous.exe");
        fs::copy(&plan.staged, &replacement).map_err(|e| {
            format!("Cannot write beside the executable. Move WindowTabs to a writable folder: {e}")
        })?;
        if hash_file(&replacement)? != plan.sha256 {
            return Err("Staged update changed; installation stopped.".into());
        }
        fs::copy(&plan.target, &backup).map_err(|e| format!("Cannot create rollback copy: {e}"))?;
        storage::replace_file(&replacement, &plan.target)?;
        if let Err(error) = std::process::Command::new(&plan.target).spawn() {
            let rollback = storage::replace_file(&backup, &plan.target);
            return Err(format!(
                "Could not restart: {error}. Rollback: {rollback:?}"
            ));
        }
        let _ = fs::remove_file(&plan.staged);
        let _ = fs::remove_file(plan_path);
        Ok(())
    }
}
