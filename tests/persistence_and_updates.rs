//! Filesystem fixtures stay in this repository's target directory. No Win32 or network calls.
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};
use window_tabs::{
    storage::{self, Settings, Store},
    update::*,
};

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("test-fixtures")
            .join(format!(
                "{}-{}",
                std::process::id(),
                SEQUENCE.fetch_add(1, Ordering::Relaxed)
            ));
        fs::create_dir_all(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }
    fn pe(&self, name: &str, machine: u16) -> PathBuf {
        let path = self.0.join(name);
        let mut bytes = vec![0u8; 1024];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&128u32.to_le_bytes());
        bytes[128..132].copy_from_slice(b"PE\0\0");
        bytes[132..134].copy_from_slice(&machine.to_le_bytes());
        bytes[134..136].copy_from_slice(&1u16.to_le_bytes());
        bytes[148..150].copy_from_slice(&240u16.to_le_bytes());
        bytes[150..152].copy_from_slice(&0x0022u16.to_le_bytes());
        bytes[152..154].copy_from_slice(&0x020bu16.to_le_bytes());
        bytes[168..172].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[208..212].copy_from_slice(&0x2000u32.to_le_bytes());
        bytes[212..216].copy_from_slice(&512u32.to_le_bytes());
        bytes[220..222].copy_from_slice(&2u16.to_le_bytes());
        bytes[392..397].copy_from_slice(b".text");
        bytes[400..404].copy_from_slice(&16u32.to_le_bytes());
        bytes[404..408].copy_from_slice(&0x1000u32.to_le_bytes());
        bytes[408..412].copy_from_slice(&512u32.to_le_bytes());
        bytes[412..416].copy_from_slice(&512u32.to_le_bytes());
        bytes[428..432].copy_from_slice(&0x60000020u32.to_le_bytes());
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn release(version: &str) -> Value {
    let filename = format!("WindowTabs-v{version}.exe");
    json!({"tag_name":format!("v{version}"),"draft":false,"prerelease":false,"assets":[
        {"name":filename,"browser_download_url":format!("https://github.com/siarheisinelnikau/window-tabs/releases/download/v{version}/{filename}"),"size":1000},
        {"name":"SHA256SUMS.txt","browser_download_url":format!("https://github.com/siarheisinelnikau/window-tabs/releases/download/v{version}/SHA256SUMS.txt"),"size":100}
    ]})
}
fn select(value: &Value, current: &str) -> window_tabs::model::Result<Option<Available>> {
    select_release(&serde_json::to_vec(value).unwrap(), current)
}

#[test]
fn versions_use_semantic_precedence_and_stable_channel() {
    assert!(select(&release("0.10.0"), "0.9.0").unwrap().is_some());
    assert!(select(&release("0.9.0"), "0.10.0").unwrap().is_none());
    assert!(
        select(&release("0.1.0+build2"), "0.1.0+build1")
            .unwrap()
            .is_none()
    );
    let mut value = release("0.2.0");
    value["draft"] = json!(true);
    assert!(select(&value, "0.1.0").unwrap().is_none());
    value["draft"] = json!(false);
    value["prerelease"] = json!(true);
    assert!(select(&value, "0.1.0").unwrap().is_none());
    assert!(select(&release("0.2.0-beta.1"), "0.1.0").unwrap().is_none());
}
#[test]
fn foreign_asset_missing_checksum_and_wrong_package_are_errors() {
    let mut value = release("0.2.0");
    value["assets"][0]["browser_download_url"] =
        json!("https://example.invalid/WindowTabs-v0.2.0.exe");
    assert!(select(&value, "0.1.0").is_err());
    value = release("0.2.0");
    value["assets"].as_array_mut().unwrap().pop();
    assert!(select(&value, "0.1.0").is_err());
    value = release("0.2.0");
    value["assets"][0]["name"] = json!("WindowTabs-v0.2.0-arm64.exe");
    assert!(select(&value, "0.1.0").is_err());
    value = release("0.2.0");
    value["assets"][0]["size"] = json!(MAX_EXE + 1);
    assert!(select(&value, "0.1.0").is_err());
    assert!(select_release(b"not JSON", "0.1.0").is_err());
}
#[test]
fn checksum_parser_requires_one_exact_complete_hash() {
    let name = "WindowTabs-v0.2.0.exe";
    let hash = "aB".repeat(32);
    assert_eq!(
        checksum(&format!("{hash}  {name}\r\n"), name).unwrap(),
        hash.to_lowercase()
    );
    assert!(checksum(&format!("{hash}  wrong.exe"), name).is_err());
    assert!(checksum(&format!("abc  {name}"), name).is_err());
    assert!(checksum(&format!("{hash}  {name}\n{hash}  {name}"), name).is_err());
    assert!(checksum(&format!("{hash}  {name} trailing"), name).is_err());
}
#[test]
fn payload_validation_rejects_truncation_wrong_architecture_and_tampering() {
    let fixture = Fixture::new();
    let staged = fixture.pe("payload.exe", 0x8664);
    let helper = fixture.0.join("WindowTabs-updater.exe");
    let target = fixture.0.join("installed").join("WindowTabs.exe");
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::copy(&staged, &target).unwrap();
    fs::copy(&staged, &helper).unwrap();
    let plan = InstallPlan {
        parent_pid: 123,
        target: fs::canonicalize(target).unwrap(),
        staged: staged.clone(),
        sha256: hash_file(&staged).unwrap(),
        original_sha256: hash_file(&helper).unwrap(),
        transaction_id: "a".repeat(32),
    };
    // A helper cannot install into its own temporary directory tree.
    assert!(plan.validate(&helper).is_err());
    let temporary = fixture.0.join("download");
    fs::create_dir(&temporary).unwrap();
    let helper = temporary.join("WindowTabs-updater.exe");
    fs::copy(&staged, &helper).unwrap();
    let downloaded = temporary.join("payload.exe");
    fs::copy(&staged, &downloaded).unwrap();
    let plan = InstallPlan {
        staged: fs::canonicalize(downloaded).unwrap(),
        ..plan
    };
    plan.validate(&helper).unwrap();
    fs::write(&plan.staged, b"partial file").unwrap();
    assert!(plan.validate(&helper).is_err());
    assert!(verify_pe(&fixture.pe("arm.exe", 0xaa64)).is_err());
    let mut plan = plan;
    plan.staged = fixture.0.join("elsewhere").join("payload.exe");
    assert!(plan.validate(&helper).is_err());
}

#[test]
fn executable_headers_cannot_mask_truncated_data_or_a_dll() {
    let fixture = Fixture::new();
    let path = fixture.pe("truncated.exe", 0x8664);
    let mut bytes = fs::read(&path).unwrap();
    bytes.truncate(600);
    fs::write(&path, &bytes).unwrap();
    assert!(verify_pe(&path).is_err());
    let path = fixture.pe("library.exe", 0x8664);
    let mut bytes = fs::read(&path).unwrap();
    bytes[150..152].copy_from_slice(&0x2022u16.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(verify_pe(&path).is_err());
    let path = fixture.pe("console.exe", 0x8664);
    let mut bytes = fs::read(&path).unwrap();
    bytes[220..222].copy_from_slice(&3u16.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(verify_pe(&path).is_err());
    let path = fixture.pe("entry.exe", 0x8664);
    let mut bytes = fs::read(&path).unwrap();
    bytes[168..172].copy_from_slice(&0x1800u32.to_le_bytes());
    fs::write(&path, &bytes).unwrap();
    assert!(verify_pe(&path).is_err());
}

#[test]
fn installation_plan_rejects_a_changed_current_executable_and_path_components() {
    let fixture = Fixture::new();
    fs::create_dir(fixture.0.join("download")).unwrap();
    let target = fixture.pe("WindowTabs.exe", 0x8664);
    let staged = fixture.pe("download/payload.exe", 0x8664);
    let helper = fixture.pe("download/WindowTabs-updater.exe", 0x8664);
    let mut plan = InstallPlan {
        parent_pid: 123,
        target: target.clone(),
        staged: staged.clone(),
        sha256: hash_file(&staged).unwrap(),
        original_sha256: hash_file(&target).unwrap(),
        transaction_id: "b".repeat(32),
    };
    plan.validate(&helper).unwrap();
    plan.transaction_id = "../escape".into();
    assert!(plan.validate(&helper).is_err());
    plan.transaction_id = "b".repeat(32);
    fs::write(&target, b"another installation").unwrap();
    assert!(plan.validate(&helper).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"another installation");
}

#[test]
fn previous_journal_schema_without_physical_coordinates_remains_readable() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone());
    let rectangle = json!({"left":0,"top":0,"right":640,"bottom":480});
    storage::atomic_json(&fixture.0.join("recovery.json"),&json!({"schema":1,"originals":[{
        "info":{"identity":{"handle":1,"pid":1,"started":1,"class":"Fixture"},"title":"Fixture","executable":"fixture.exe","bounds":rectangle},
        "placement":{"flags":0,"show":1,"min":[-1,-1],"max":[-1,-1],"normal":rectangle},"visible":true
    }]})).unwrap();
    let originals = store.recovery().unwrap();
    assert_eq!(originals.len(), 1);
    assert!(originals[0].normal_screen.is_none());
    assert_eq!(originals[0].placement.normal.right, 640);
}
#[test]
fn settings_opt_out_survives_atomic_replacement() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone());
    assert!(store.settings().unwrap().check_on_startup);
    let settings = Settings {
        check_on_startup: false,
        ..Default::default()
    };
    store.save_settings(&settings).unwrap();
    assert!(!store.settings().unwrap().check_on_startup);
    store.save_settings(&Settings::default()).unwrap();
    assert!(store.settings().unwrap().check_on_startup);
    assert!(!fixture.0.join("settings.json.tmp").exists());
}
#[test]
fn malformed_and_future_settings_are_preserved_and_reported() {
    let fixture = Fixture::new();
    let store = Store::new(fixture.0.clone());
    let path = fixture.0.join("settings.json");
    fs::write(&path, b"broken{").unwrap();
    assert!(store.settings().is_err());
    assert_eq!(fs::read(&path).unwrap(), b"broken{");
    storage::atomic_json(&path, &json!({"schema":99})).unwrap();
    assert!(store.settings().is_err());
    fs::write(
        fixture.0.join("recovery.json"),
        b"{\"schema\":99,\"originals\":[]}",
    )
    .unwrap();
    assert!(store.recovery().is_err());
}
