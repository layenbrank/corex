use glob::Pattern;
use std::path::Path;

/// 创建新的忽略处理器
pub fn build(ignore_patterns: &[String]) -> Vec<Pattern> {
    let patterns = ignore_patterns
        .iter()
        .filter_map(|p| Pattern::new(p).ok())
        .collect();
    patterns
}

/// 检查路径是否被忽略
pub fn ignored(patterns: &Vec<Pattern>, path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // 检查完整路径
    if patterns.iter().any(|p| p.matches(&path_str)) {
        return true;
    }

    // 检查文件名
    if let Some(filename) = path.file_name() {
        let filename_str = filename.to_string_lossy();
        if patterns.iter().any(|p| p.matches(&filename_str)) {
            return true;
        }
    }

    // 检查路径组件
    path.components()
        .map(|c| c.as_os_str().to_string_lossy())
        .any(|component| patterns.iter().any(|p| p.matches(&component)))
}
