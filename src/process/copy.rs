use anyhow::{Context, Result};
use glob::Pattern;
use std::{fs, path::Path};
use walkdir::WalkDir;

// 不能在常量中直接使用 Vec，因为 Vec 的分配是在堆上完成的，
// 而常量要求所有内容在编译期就确定且存储在只读内存中。
// 你可以使用数组（如 IGNORES_VEC），在需要 Vec 时再转换：

pub fn process_copy(source: &Path, target: &Path, ignores: Vec<String>) -> Result<()> {
    // 确保目标目录存在
    if !target.exists() {
        fs::create_dir_all(target).context("创建目录失败")?;
    }

    // 预编译 glob 模式以提高性能
    let ignore_patterns: Vec<Pattern> = ignores
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect();

    // 递归遍历源目录
    for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
        let source_path = entry.path();

        let raw_path = source_path.strip_prefix(source).context("路径解析失败")?;

        let target_path = target.join(raw_path);

        if is_ignored(&raw_path, &ignore_patterns) {
            println!("忽略路径: {:?}", raw_path);
            continue;
        }

        // 处理子目录
        if source_path.is_dir() {
            if !target_path.exists() {
                fs::create_dir(target_path).context("创建目录失败")?;
            }
        }
        // 复制文件
        else if source_path.is_file() {
            if let Some(parent) = target_path.parent() {
                if parent.exists() {
                    fs::create_dir_all(parent).context("创建父目录失败")?;
                }
            }
            fs::copy(source_path, &target_path).context(format!(
                "复制文件失败: {:?} -> {:?}",
                source_path, target_path
            ))?;
        }
    }

    Ok(())
}

fn is_ignored(path: &Path, patterns: &[Pattern]) -> bool {
    let path_str = path.to_string_lossy();

    for pattern in patterns {
        if pattern.matches(&path_str) {
            return true;
        }
        if let Some(filename) = path.file_name() {
            if pattern.matches(&filename.to_string_lossy()) {
                return true;
            }
        }

        for component in path.components() {
            if pattern.matches(&component.as_os_str().to_string_lossy()) {
                return true;
            }
        }
    }

    false

    // 检查路径是否与任何忽略模式匹配
    // ignore_patterns
    //     .iter()
    //     .any(|pattern| pattern.matches(&path_str))
}
