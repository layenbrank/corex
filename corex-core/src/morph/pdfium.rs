//! Pdfium 原生库动态加载（仅捆绑目录，不回退系统库）。

use std::path::PathBuf;

use pdfium_render::prelude::*;

/// 与 `assets/pdfium/VERSION` 及 `pdfium-render` `pdfium_latest` 对齐。
pub fn version() -> &'static str {
    include_str!("../../../assets/pdfium/VERSION").trim()
}

/// 查找顺序：可执行文件目录 → `COREX_PDFIUM_DIR`。
pub fn load() -> Result<Box<dyn PdfiumLibraryBindings>, String> {
    let dirs = lib_dirs();
    let mut last_err = None;

    for dir in &dirs {
        let path = Pdfium::pdfium_platform_library_name_at_path(dir);
        match Pdfium::bind_to_library(&path) {
            Ok(bindings) => return Ok(bindings),
            Err(e) => last_err = Some(e.to_string()),
        }
    }

    let searched = dirs
        .iter()
        .map(|d| d.display().to_string())
        .collect::<Vec<_>>()
        .join(", ");

    Err(format!(
        "pdfium.dll not found (chromium/{}); searched: [{}]; {}",
        version(),
        searched,
        last_err.unwrap_or_else(|| "no search paths".into())
    ))
}

fn lib_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        dirs.push(dir.to_path_buf());
    }

    if let Ok(dir) = std::env::var("COREX_PDFIUM_DIR") {
        dirs.push(PathBuf::from(dir));
    }

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_numeric_build_id() {
        let v = version();
        assert!(!v.is_empty());
        assert!(v.chars().all(|c| c.is_ascii_digit()));
    }
}
