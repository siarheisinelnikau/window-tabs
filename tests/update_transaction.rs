//! No native updater, processes, network, or actual installation files are used.
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};
use window_tabs::{
    install::{InstallIo, replace_and_restart},
    model::Result,
    update::InstallPlan,
};

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
#[derive(Default)]
struct MemoryIo {
    files: HashMap<PathBuf, Vec<u8>>,
    launches: Vec<Vec<u8>>,
    fail_launches: usize,
    fail_copy: HashSet<PathBuf>,
    fail_replaces: HashSet<usize>,
    replaces: usize,
    mutate_after_copy: Option<(PathBuf, PathBuf)>,
}
impl InstallIo for MemoryIo {
    fn hash(&mut self, path: &Path) -> Result<String> {
        self.files
            .get(path)
            .map(|bytes| hash(bytes))
            .ok_or("Missing file".into())
    }
    fn copy_new(&mut self, from: &Path, to: &Path) -> Result<()> {
        if self.files.contains_key(to) || self.fail_copy.contains(to) {
            return Err("Cannot create file".into());
        }
        let bytes = self.files.get(from).ok_or("Missing source")?.clone();
        self.files.insert(to.to_path_buf(), bytes);
        if let Some((copied, target)) = &self.mutate_after_copy
            && copied == to
        {
            self.files
                .insert(target.clone(), b"changed by another installation".to_vec());
        }
        Ok(())
    }
    fn replace(&mut self, from: &Path, to: &Path) -> Result<()> {
        self.replaces += 1;
        if self.fail_replaces.contains(&self.replaces) {
            return Err("Replacement failed".into());
        }
        let bytes = self.files.remove(from).ok_or("Missing replacement")?;
        self.files.insert(to.to_path_buf(), bytes);
        Ok(())
    }
    fn remove(&mut self, path: &Path) -> Result<()> {
        self.files.remove(path);
        Ok(())
    }
    fn restart(&mut self, path: &Path) -> Result<()> {
        self.launches
            .push(self.files.get(path).ok_or("Missing executable")?.clone());
        if self.launches.len() <= self.fail_launches {
            Err("Launch failed".into())
        } else {
            Ok(())
        }
    }
}
fn setup() -> (InstallPlan, MemoryIo) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("virtual-install");
    let plan = InstallPlan {
        parent_pid: 123,
        target: root.join("app").join("WindowTabs.exe"),
        staged: root.join("download").join("payload.exe"),
        sha256: hash(b"new executable"),
        original_sha256: hash(b"old executable"),
        transaction_id: "a".repeat(32),
    };
    let mut io = MemoryIo::default();
    io.files
        .insert(plan.target.clone(), b"old executable".to_vec());
    io.files
        .insert(plan.staged.clone(), b"new executable".to_vec());
    (plan, io)
}
#[test]
fn verified_replacement_keeps_a_rollback_copy_and_restarts_new_bytes() {
    let (plan, mut io) = setup();
    let (stage, backup) = plan.transaction_paths().unwrap();
    assert_eq!(replace_and_restart(&plan, &mut io).unwrap(), backup);
    assert_eq!(io.files[&plan.target], b"new executable");
    assert_eq!(io.files[&backup], b"old executable");
    assert!(!io.files.contains_key(&stage));
    assert_eq!(io.launches, vec![b"new executable".to_vec()]);
}
#[test]
fn changed_target_or_payload_is_never_replaced() {
    let (plan, mut io) = setup();
    io.files
        .insert(plan.target.clone(), b"different installed version".to_vec());
    assert!(replace_and_restart(&plan, &mut io).is_err());
    assert_eq!(io.replaces, 0);
    assert_eq!(io.files[&plan.target], b"different installed version");
    let (plan, mut io) = setup();
    io.files.insert(plan.staged.clone(), b"truncated".to_vec());
    assert!(replace_and_restart(&plan, &mut io).is_err());
    assert_eq!(io.replaces, 0);
    assert_eq!(io.files[&plan.target], b"old executable");
}
#[test]
fn existing_sidecars_are_not_overwritten_or_deleted() {
    for collision_is_backup in [false, true] {
        let (plan, mut io) = setup();
        let (stage, backup) = plan.transaction_paths().unwrap();
        let collision = if collision_is_backup {
            backup
        } else {
            stage.clone()
        };
        io.files
            .insert(collision.clone(), b"existing file".to_vec());
        assert!(replace_and_restart(&plan, &mut io).is_err());
        assert_eq!(io.files[&collision], b"existing file");
        assert_eq!(io.files[&plan.target], b"old executable");
        assert_eq!(io.replaces, 0);
        if collision_is_backup {
            assert!(!io.files.contains_key(&stage));
        }
    }
}
#[test]
fn backup_or_replacement_failure_preserves_the_installed_executable() {
    let (plan, mut io) = setup();
    let (stage, backup) = plan.transaction_paths().unwrap();
    io.fail_copy.insert(backup);
    assert!(replace_and_restart(&plan, &mut io).is_err());
    assert_eq!(io.files[&plan.target], b"old executable");
    assert!(!io.files.contains_key(&stage));
    let (plan, mut io) = setup();
    io.fail_replaces.insert(1);
    assert!(replace_and_restart(&plan, &mut io).is_err());
    assert_eq!(io.files[&plan.target], b"old executable");
    assert!(io.launches.is_empty());
}
#[test]
fn launch_failure_restores_and_restarts_the_previous_executable() {
    let (plan, mut io) = setup();
    let (_, backup) = plan.transaction_paths().unwrap();
    io.fail_launches = 1;
    let error = replace_and_restart(&plan, &mut io).unwrap_err();
    assert!(error.contains("previous executable was restored"));
    assert_eq!(io.files[&plan.target], b"old executable");
    assert_eq!(io.files[&backup], b"old executable");
    assert_eq!(
        io.launches,
        vec![b"new executable".to_vec(), b"old executable".to_vec()]
    );
}
#[test]
fn rollback_failure_retains_the_verified_backup_and_reports_its_path() {
    let (plan, mut io) = setup();
    let (_, backup) = plan.transaction_paths().unwrap();
    io.fail_launches = 1;
    io.fail_replaces.insert(2);
    let error = replace_and_restart(&plan, &mut io).unwrap_err();
    assert!(error.contains("Rollback failed"));
    assert!(error.contains(backup.to_str().unwrap()));
    assert_eq!(io.files[&backup], b"old executable");
    assert_eq!(io.files[&plan.target], b"new executable");
    assert_eq!(io.launches.len(), 1);
}
#[test]
fn rollback_launch_failure_still_restores_the_previous_file() {
    let (plan, mut io) = setup();
    io.fail_launches = 2;
    let error = replace_and_restart(&plan, &mut io).unwrap_err();
    assert!(error.contains("Previous-version restart: failed"));
    assert_eq!(io.files[&plan.target], b"old executable");
}
#[test]
fn concurrent_target_change_after_backup_aborts_without_overwriting_it() {
    let (plan, mut io) = setup();
    let (_, backup) = plan.transaction_paths().unwrap();
    io.mutate_after_copy = Some((backup, plan.target.clone()));
    assert!(replace_and_restart(&plan, &mut io).is_err());
    assert_eq!(io.replaces, 0);
    assert_eq!(io.files[&plan.target], b"changed by another installation");
}
