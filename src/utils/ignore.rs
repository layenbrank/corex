use glob::Pattern;
use std::path::Path;

/// 统一的忽略模式处理
pub struct Ignore {
    patterns: Vec<Pattern>,
}

impl Ignore {
    /// 创建新的忽略处理器
    pub fn new(ignore_patterns: &[String]) -> Self {
        let patterns = ignore_patterns
            .iter()
            .filter_map(|p| Pattern::new(p).ok())
            .collect();

        Self { patterns }
    }

    /// 检查路径是否被忽略
    pub fn ignored(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        // 检查完整路径
        if self.patterns.iter().any(|p| p.matches(&path_str)) {
            return true;
        }

        // 检查文件名
        if let Some(filename) = path.file_name() {
            let filename_str = filename.to_string_lossy();
            if self.patterns.iter().any(|p| p.matches(&filename_str)) {
                return true;
            }
        }

        // 检查路径组件
        path.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .any(|component| self.patterns.iter().any(|p| p.matches(&component)))
    }

    /// 获取编译后的模式
    pub fn patterns(&self) -> &[Pattern] {
        &self.patterns
    }
}
