//! Path glob matching for watch events.

use glob::Pattern;

/// Returns true when `rel_path` passes include/exclude filters.
pub fn path_matches(rel_path: &str, includes: &[String], excludes: &[String]) -> bool {
    if !excludes.is_empty() {
        for pat in excludes {
            if Pattern::new(pat)
                .map(|p| p.matches(rel_path))
                .unwrap_or(false)
            {
                return false;
            }
        }
    }
    if includes.is_empty() {
        return true;
    }
    includes.iter().any(|pat| {
        Pattern::new(pat)
            .map(|p| p.matches(rel_path))
            .unwrap_or(false)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclude_node_modules() {
        assert!(!path_matches(
            "src/node_modules/pkg/a.rs",
            &[],
            &["**/node_modules/**".into()]
        ));
        assert!(path_matches("src/main.rs", &[], &["**/node_modules/**".into()]));
    }
}
