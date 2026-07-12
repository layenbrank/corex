//! 构建期确保 `assets/pdfium/{target}/pdfium.dll` 存在，并复制到可执行文件输出目录。

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn copy_pdfium_dll(manifest_dir: &Path) {
    let workspace = manifest_dir.parent().expect("workspace root");
    let target = env::var("TARGET").expect("TARGET");
    let profile = env::var("PROFILE").expect("PROFILE");

    let target_root = env::var("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| workspace.join("target"));

    let src = workspace
        .join("assets")
        .join("pdfium")
        .join(&target)
        .join("pdfium.dll");

    let dst = target_root.join(&target).join(&profile).join("pdfium.dll");
    let version = workspace.join("assets/pdfium/VERSION");
    let checksum = workspace.join("assets/pdfium/pdfium-win-x64.tgz.sha256");
    let script = workspace.join("scripts/download-pdfium.ps1");

    println!("cargo:rerun-if-changed={}", src.display());
    println!("cargo:rerun-if-changed={}", version.display());
    println!("cargo:rerun-if-changed={}", checksum.display());
    println!("cargo:rerun-if-changed={}", script.display());

    if !src.exists() {
        ensure_pdfium_dll(workspace, &src, &script);
    }

    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent).expect("create target output dir");
    }
    fs::copy(&src, &dst).expect("copy pdfium.dll to output dir");
}

fn ensure_pdfium_dll(workspace: &Path, src: &Path, script: &Path) {
    if !script.exists() {
        panic!(
            "pdfium.dll not found at {} and download script missing at {}",
            src.display(),
            script.display()
        );
    }

    println!(
        "cargo:warning=pdfium.dll missing; running {}",
        script.display()
    );

    let status = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(script)
        .current_dir(workspace)
        .status()
        .unwrap_or_else(|e| {
            panic!(
                "failed to run {}: {e}\nInstall PowerShell 7 (pwsh) or run the script manually",
                script.display()
            )
        });

    if !status.success() {
        panic!(
            "scripts/download-pdfium.ps1 failed with status {status}; pdfium.dll still required at {}",
            src.display()
        );
    }

    if !src.exists() {
        panic!(
            "download script completed but pdfium.dll not found at {}",
            src.display()
        );
    }
}
