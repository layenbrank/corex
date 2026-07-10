use std::path::Path;

use anyhow::{Context, Result, bail};

/// 校验可读文件路径存在
pub fn validate_read_file(path: &str) -> Result<()> {
    let p = Path::new(path);
    if !p.is_file() {
        bail!("未找到指定文件: {path}");
    }
    Ok(())
}

/// 校验可读路径存在（文件或目录）
pub fn validate_read_path(path: &str) -> Result<()> {
    if path == "." || Path::new(path).exists() {
        Ok(())
    } else {
        bail!("未找到指定路径: {path}");
    }
}

/// 校验目录存在
pub fn validate_dir(path: &str) -> Result<()> {
    if Path::new(path).is_dir() {
        Ok(())
    } else {
        bail!("未找到指定目录: {path}");
    }
}

/// 校验输出路径：若父目录不存在则创建
pub fn validate_write_path(path: &str) -> Result<()> {
    let p = Path::new(path);
    if let Some(parent) = p.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("创建输出目录失败: {}", parent.display()))?;
        }
    }
    Ok(())
}

/// 校验输出目录（不存在则创建）
pub fn validate_output_dir(path: &str) -> Result<()> {
    std::fs::create_dir_all(path).with_context(|| format!("创建输出目录失败: {path}"))?;
    Ok(())
}
