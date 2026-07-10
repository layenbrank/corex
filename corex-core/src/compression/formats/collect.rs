use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use walkdir::{DirEntry, WalkDir};

use crate::utils::Filter;

/// 收集目录下待打包的文件（相对路径 + 绝对路径）
pub fn collect_files(
    from: &Path,
    includes: &[String],
    excludes: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>> {
    if !from.is_dir() {
        bail!("源路径必须是目录: {}", from.display());
    }

    let filter = Filter::new(includes, excludes);
    let spinner = crate::utils::progress::spinner("正在扫描文件...");
    let entries: Vec<(PathBuf, PathBuf)> = WalkDir::new(from)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .strip_prefix(from)
                .map(|rel| !filter.is_filtered(rel))
                .unwrap_or(true)
        })
        .filter_map(|e| {
            e.path()
                .strip_prefix(from)
                .ok()
                .map(|rel| (rel.to_path_buf(), e.path().to_path_buf()))
        })
        .collect();
    spinner.finish_and_clear();

    Ok(entries)
}

/// 保留 DirEntry 列表供 zip 进度条使用
pub fn collect_dir_entries(
    from: &Path,
    includes: &[String],
    excludes: &[String],
) -> Result<Vec<DirEntry>> {
    if !from.is_dir() {
        bail!("源路径必须是目录: {}", from.display());
    }

    let filter = Filter::new(includes, excludes);
    Ok(WalkDir::new(from)
        .min_depth(1)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter(|e| {
            e.path()
                .strip_prefix(from)
                .map(|rel| !filter.is_filtered(rel))
                .unwrap_or(true)
        })
        .collect())
}
