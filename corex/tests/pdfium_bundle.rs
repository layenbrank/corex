//! 验证 `pdfium.dll` 与 `corex.exe` 同目录（构建期由 build.rs 复制）。

use assert_cmd::Command;
use std::path::PathBuf;

#[test]
fn pdfium_dll_beside_corex() {
    let corex = Command::cargo_bin("corex").expect("locate corex binary");
    let exe = PathBuf::from(corex.get_program());
    let dir = exe.parent().expect("corex parent directory");
    let dll = dir.join("pdfium.dll");

    assert!(
        dll.exists(),
        "expected pdfium.dll beside corex at {}; run scripts/download-pdfium.ps1 then rebuild",
        dll.display()
    );
}
