//! Glob include/exclude filter (ported from corex-core utils).

use glob::Pattern;
use std::path::Path;

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub includes: Vec<Pattern>,
    pub excludes: Vec<Pattern>,
}

impl Filter {
    pub fn new(includes: &[String], excludes: &[String]) -> Self {
        Self {
            includes: parse_patterns(includes),
            excludes: parse_patterns(excludes),
        }
    }

    /// `true` = skip this path.
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
        .filter_map(|p| Pattern::new(p).ok())
        .collect()
}
