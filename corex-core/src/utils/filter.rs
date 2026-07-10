use glob::Pattern;
use std::path::Path;

/// 过滤规则：包含 include（白名单）和 exclude（黑名单）
#[derive(Debug, Clone, Default)]
pub struct Filter {
    /// 白名单模式（非空时仅匹配白名单的文件）
    pub includes: Vec<Pattern>,
    /// 黑名单模式（匹配的文件被排除）
    pub excludes: Vec<Pattern>,
}

impl Filter {
    /// 从字符串列表构建过滤器
    ///
    /// 无效的 glob 模式会被跳过并输出警告。
    pub fn new(includes: &[String], excludes: &[String]) -> Self {
        Self {
            includes: parse_patterns(includes),
            excludes: parse_patterns(excludes),
        }
    }

    /// 检查路径是否被过滤（true = 应跳过）
    ///
    /// 逻辑：
    /// 1. 如果 include 列表非空，路径必须匹配至少一个 include 模式
    /// 2. 如果路径匹配任意一个 exclude 模式，则被排除
    pub fn is_filtered(&self, path: &Path) -> bool {
        if !self.includes.is_empty() && !self.matches_any(&self.includes, path) {
            return true;
        }

        self.matches_any(&self.excludes, path)
    }

    fn matches_any(&self, patterns: &[Pattern], path: &Path) -> bool {
        let path_str = path.to_string_lossy();

        if patterns.iter().any(|p| p.matches(&path_str)) {
            return true;
        }

        if let Some(filename) = path.file_name() {
            let filename_str = filename.to_string_lossy();
            if patterns.iter().any(|p| p.matches(&filename_str)) {
                return true;
            }
        }

        path.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .any(|component| patterns.iter().any(|p| p.matches(&component)))
    }
}

fn parse_patterns(patterns: &[String]) -> Vec<Pattern> {
    patterns
        .iter()
        .filter_map(|p| match Pattern::new(p) {
            Ok(pat) => Some(pat),
            Err(e) => {
                eprintln!("⚠️  无效的过滤模式 '{}': {}", p, e);
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn exclude_filters_matching_paths() {
        let filter = Filter::new(&[], &["**/*.tmp".to_string()]);
        assert!(filter.is_filtered(Path::new("foo/bar.tmp")));
        assert!(!filter.is_filtered(Path::new("foo/bar.rs")));
    }

    #[test]
    fn include_whitelist_only() {
        let filter = Filter::new(&["**/*.rs".to_string()], &[]);
        assert!(!filter.is_filtered(Path::new("main.rs")));
        assert!(filter.is_filtered(Path::new("main.js")));
    }
}
