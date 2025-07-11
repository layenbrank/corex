use std::{fs, path::Path};

use anyhow::{Context, Result};
// use glob::glob;
use walkdir::WalkDir;

pub fn process_copy(source: &Path, target: &Path, ignores: Vec<String>) -> Result<()> {
    // 确保目标目录存在
    if !target.exists() {
        fs::create_dir_all(target).context("创建目录失败")?;
    }

    // 递归遍历源目录
    for entry in WalkDir::new(source).into_iter().filter_map(|e| e.ok()) {
        let source_path = entry.path();

        println!("{:?}", source_path);

        let real_path = source_path.strip_prefix(source).context("路径解析失败")?;

        let target_path = target.join(real_path);

        // glob(source_path.to_str().expect("匹配失败"))?;

        // 处理子目录
        if source_path.is_dir() {
            if !target_path.exists() {
                fs::create_dir(target_path).context("创建目录失败")?;
            }
        }
        // 复制文件
        else if source_path.is_file() {
            fs::copy(source_path, &target_path).context(format!(
                "复制文件失败: {:?} -> {:?}",
                source_path, target_path
            ))?;
        }
    }

    Ok(())
}
