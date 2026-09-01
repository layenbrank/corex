//! Path glob matching for watch events (Vite/chokidar-style: relative path + component match).

use glob::Pattern;
use std::path::Path;

fn normalize_path_str(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Strip the first matching watch root prefix; normalize to forward slashes.
pub fn watch_relative_path(event_path: &Path, watch_roots: &[String]) -> String {
    let event = normalize_path_str(event_path);
    for root in watch_roots {
        let root_norm = normalize_path_str(Path::new(root));
        let root_prefix = root_norm.trim_end_matches('/');
        if event.eq_ignore_ascii_case(root_prefix) {
            return String::new();
        }
        let prefix = format!("{root_prefix}/");
        if event.len() >= prefix.len() && event[..prefix.len()].eq_ignore_ascii_case(&prefix) {
            return event[prefix.len()..].to_string();
        }
    }
    event_path
        .file_name()
        .map(|n| n.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default()
}

/// Compiled include/exclude filter; semantics align with `copy.run` / Vite `server.watch.ignored`.
#[derive(Debug, Clone)]
pub struct WatchFilter {
    includes: Vec<Pattern>,
    excludes: Vec<Pattern>,
}

impl WatchFilter {
    pub fn new(includes: &[String], excludes: &[String]) -> Self {
        Self {
            includes: parse_patterns(includes),
            excludes: parse_patterns(excludes),
        }
    }

    /// Returns true when the relative path should trigger a rebuild.
    pub fn matches(&self, rel_path: &str) -> bool {
        let path = Path::new(rel_path);
        if !self.includes.is_empty() && !self.matches_any(&self.includes, path) {
            return false;
        }
        !self.matches_any(&self.excludes, path)
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

/// Returns true when `rel_path` passes include/exclude filters.
pub fn path_matches(rel_path: &str, includes: &[String], excludes: &[String]) -> bool {
    WatchFilter::new(includes, excludes).matches(rel_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn exclude_node_modules() {
        assert!(!path_matches(
            "src/node_modules/pkg/a.rs",
            &[],
            &["**/node_modules/**".into()]
        ));
        assert!(path_matches(
            "src/main.rs",
            &[],
            &["**/node_modules/**".into()]
        ));
    }

    #[test]
    fn watch_relative_path_strips_root() {
        let root = r"C:\proj\iwellnew".to_string();
        let event = PathBuf::from(r"C:\proj\iwellnew\src\App.tsx");
        assert_eq!(watch_relative_path(&event, &[root]), "src/App.tsx");
    }

    #[test]
    fn include_src_directory_shorthand() {
        assert!(path_matches("src/App.tsx", &["src".into()], &[]));
        assert!(!path_matches("dist/App.js", &["src".into()], &[]));
    }

    #[test]
    fn exclude_directory_shorthand() {
        assert!(!path_matches("new/index.js", &[], &["new".into()]));
        assert!(path_matches("src/index.js", &[], &["new".into()]));
    }

    #[test]
    fn include_src_glob() {
        assert!(path_matches("src/App.tsx", &["src/**".into()], &[]));
        assert!(!path_matches("dist/App.js", &["src/**".into()], &[]));
    }
}
