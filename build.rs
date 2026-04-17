use std::{env, path::PathBuf, process::Command};

fn resource_compiler() -> PathBuf {
    let host = match env::consts::ARCH {
        "aarch64" => "arm64",
        "x86" => "x86",
        _ => "x64",
    };
    if let Some(sdk) = env::var_os("WindowsSdkVerBinPath") {
        let compiler = PathBuf::from(sdk).join(host).join("rc.exe");
        if compiler.is_file() {
            return compiler;
        }
    }
    for root in ["ProgramFiles(x86)", "ProgramFiles"] {
        let Some(root) = env::var_os(root) else {
            continue;
        };
        let bin = PathBuf::from(root).join("Windows Kits/10/bin");
        let mut versions: Vec<_> = std::fs::read_dir(bin)
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect();
        versions.sort();
        for version in versions.into_iter().rev() {
            let compiler = version.join(host).join("rc.exe");
            if compiler.is_file() {
                return compiler;
            }
        }
    }
    panic!("Windows SDK rc.exe is required to embed the application icon");
}

fn main() {
    println!("cargo:rerun-if-changed=app.manifest");
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/window-tabs.ico");
    println!("cargo:rerun-if-env-changed=WindowsSdkVerBinPath");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")
        && std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc")
    {
        let manifest = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("app.manifest");
        println!("cargo:rustc-link-arg-bin=WindowTabs=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg-bin=WindowTabs=/MANIFESTINPUT:{}",
            manifest.display()
        );
        let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
        let resource = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app.res");
        let status = Command::new(resource_compiler())
            .current_dir(root.join("assets"))
            .arg("/nologo")
            .arg("/fo")
            .arg(&resource)
            .arg("app.rc")
            .status()
            .expect("Cannot run Windows SDK resource compiler");
        assert!(
            status.success(),
            "Application icon resource compilation failed"
        );
        println!("cargo:rustc-link-arg-bin=WindowTabs={}", resource.display());
    }
}
