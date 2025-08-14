use crate::copy::controller::CopyArgs;
use anyhow::{Context, Result};
use glob::Pattern;
use std::{fs, path::Path};
use walkdir::WalkDir;

// 不能在常量中直接使用 Vec，因为 Vec 的分配是在堆上完成的，
// 而常量要求所有内容在编译期就确定且存储在只读内存中。
// 你可以使用数组（如 IGNORES_VEC），在需要 Vec 时再转换：

// pub fn run(source: &Path, target: &Path, empty: bool, ignores: Vec<String>) -> Result<()> {
pub fn run(args: &CopyArgs) -> Result<()> {
    let from = Path::new(&args.from);
    let to = Path::new(&args.to);
    let empty = args.empty;
    let ignores = args.ignores.clone();

    // 预编译 glob 模式以提高性能
    let patterns: Vec<Pattern> = ignores
        .iter()
        .filter_map(|pattern| Pattern::new(pattern).ok())
        .collect();

    // 执行复制操作
    let copy_resp = copy_task(from, to, empty, &patterns);

    copy_resp
}

fn copy_task(from: &Path, to: &Path, empty: bool, patterns: &[Pattern]) -> Result<()> {
    // 确保目标目录存在
    if !to.exists() {
        fs::create_dir_all(to).context("创建目录失败")?;
    }

    // 如果需要清空，删除目标目录下的所有内容，但保留目录本身
    if empty && to.exists() {
        empty_dir(to).context("清空目标目录内容失败")?;
    }

    // 递归遍历源目录
    for entry in WalkDir::new(from).into_iter().filter_map(|e| e.ok()) {
        let source_path = entry.path();

        let raw_path = source_path.strip_prefix(from).context("路径解析失败")?;

        let target_path = to.join(raw_path);

        if ignored(&raw_path, &patterns) {
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
                if !parent.exists() {
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

/// 清空目录内容，但保留目录本身
fn empty_dir(dir: &Path) -> Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    for entry in fs::read_dir(dir).context("读取目录失败")? {
        let entry = entry.context("读取目录项失败")?;
        let path = entry.path();

        if path.is_dir() {
            fs::remove_dir_all(&path).context(format!("删除子目录失败: {:?}", path))?;
        } else {
            fs::remove_file(&path).context(format!("删除文件失败: {:?}", path))?;
        }
    }

    Ok(())
}

fn ignored(path: &Path, patterns: &[Pattern]) -> bool {
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
