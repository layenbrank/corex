//! Path confinement helpers shared by daemon IPC and file actions.

use std::path::{Component, Path, PathBuf};

/// Error when a path resolves outside an allowed root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathConfineError(pub String);

impl std::fmt::Display for PathConfineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PathConfineError {}

/// Returns true when `path` contains a `..` component (before canonicalization).
pub fn path_has_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

/// Ensure `path` resolves under `root` (after joining relative paths).
pub fn confine_under(root: &Path, path: &Path) -> Result<PathBuf, PathConfineError> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| PathConfineError(format!("无法解析根目录 {}: {e}", root.display())))?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    let cand_canon = candidate
        .canonicalize()
        .map_err(|e| PathConfineError(format!("无法解析路径 {}: {e}", candidate.display())))?;
    if !cand_canon.starts_with(&root_canon) {
        return Err(PathConfineError(format!(
            "路径越界: {} 不在 {} 下",
            cand_canon.display(),
            root_canon.display()
        )));
    }
    Ok(normalize_separators(cand_canon))
}

/// Ensure `path` is under at least one of `roots`.
///
/// Empty `roots` disables confinement (local/dev default).
/// Missing files are checked lexically against each root.
pub fn confine_in_roots(roots: &[PathBuf], path: &Path) -> Result<PathBuf, PathConfineError> {
    if roots.is_empty() {
        return Ok(normalize_separators(path.to_path_buf()));
    }
    if path_has_traversal(path) {
        return Err(PathConfineError(format!(
            "路径不允许包含 ..: {}",
            display_path(path)
        )));
    }
    let mut last_err = None;
    for root in roots {
        match confine_under(root, path) {
            Ok(p) => return Ok(normalize_separators(p)),
            Err(e) => last_err = Some(e),
        }
        if let Ok(p) = confine_missing(root, path) {
            return Ok(normalize_separators(p));
        }
    }
    Err(last_err.unwrap_or_else(|| {
        PathConfineError(format!(
            "路径不在 filesystem_roots 内: {}",
            display_path(path)
        ))
    }))
}

/// Normalize path separators for display and JSON (`\` on Windows).
pub fn normalize_separators(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if s.starts_with(r"\\?\") || s.starts_with("//") {
            return path;
        }
        if s.contains('/') {
            return PathBuf::from(s.replace('/', "\\"));
        }
    }
    path
}

/// Display path with platform-native separators.
pub fn display_path(path: &Path) -> String {
    normalize_separators(path.to_path_buf())
        .display()
        .to_string()
}

/// Strip Windows verbatim (`\\?\`) prefixes so paths work with `cmd` / PowerShell.
///
/// `canonicalize` often yields `\\?\C:\...`, which `cmd.exe` rejects as "path not found".
/// No-op on non-Windows and when the prefix is absent.
pub fn for_external_process(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let s = path.to_string_lossy();
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

/// Confine a possibly non-existent path under `root`.
fn confine_missing(root: &Path, path: &Path) -> Result<PathBuf, PathConfineError> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| PathConfineError(format!("无法解析根目录 {}: {e}", root.display())))?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    // Walk up until an existing ancestor is found.
    let mut cur = candidate.clone();
    let mut missing = Vec::new();
    while !cur.exists() {
        let name = cur
            .file_name()
            .ok_or_else(|| PathConfineError(format!("无法解析路径 {}", candidate.display())))?
            .to_os_string();
        missing.push(name);
        cur = cur
            .parent()
            .ok_or_else(|| PathConfineError(format!("无法解析路径 {}", candidate.display())))?
            .to_path_buf();
    }
    let mut resolved = cur
        .canonicalize()
        .map_err(|e| PathConfineError(format!("无法解析路径 {}: {e}", cur.display())))?;
    for part in missing.into_iter().rev() {
        resolved.push(part);
    }
    if !resolved.starts_with(&root_canon) {
        return Err(PathConfineError(format!(
            "路径越界: {} 不在 {} 下",
            resolved.display(),
            root_canon.display()
        )));
    }
    Ok(normalize_separators(resolved))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn detects_parent_dir() {
        assert!(path_has_traversal(Path::new("../etc/passwd")));
        assert!(!path_has_traversal(Path::new("foo/bar")));
    }

    #[test]
    fn confine_rejects_escape() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let err = confine_under(&root, Path::new("../outside")).unwrap_err();
        assert!(err.0.contains("越界") || err.0.contains("无法解析"));
    }

    #[test]
    fn empty_roots_allows_any() {
        let p = PathBuf::from("/tmp/x");
        assert_eq!(confine_in_roots(&[], &p).unwrap(), p);
    }

    #[test]
    fn roots_allow_missing_under_root() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let target = root.join("new.txt");
        let got = confine_in_roots(&[root.clone()], &target).unwrap();
        assert!(got.starts_with(root.canonicalize().unwrap()) || got == target);
    }

    #[test]
    fn roots_reject_outside() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&outside).unwrap();
        let file = outside.join("x.txt");
        fs::write(&file, b"x").unwrap();
        let err = confine_in_roots(&[root], &file).unwrap_err();
        assert!(
            err.0.contains("越界") || err.0.contains("不在") || err.0.contains("无法解析"),
            "got: {}",
            err.0
        );
    }

    #[test]
    fn normalize_mixed_separators_on_windows() {
        #[cfg(windows)]
        {
            let mixed = PathBuf::from(r"C:\Users\iwell/Documents/foo/bar");
            assert_eq!(
                normalize_separators(mixed),
                PathBuf::from(r"C:\Users\iwell\Documents\foo\bar")
            );
        }
    }

    #[test]
    fn for_external_process_strips_verbatim_prefix() {
        #[cfg(windows)]
        {
            let p = PathBuf::from(r"\\?\C:\ProgramData\corex\data\t.bat");
            assert_eq!(
                for_external_process(p),
                PathBuf::from(r"C:\ProgramData\corex\data\t.bat")
            );
            let unc = PathBuf::from(r"\\?\UNC\server\share\a.bat");
            assert_eq!(
                for_external_process(unc),
                PathBuf::from(r"\\server\share\a.bat")
            );
        }
        #[cfg(not(windows))]
        {
            let p = PathBuf::from("/tmp/x.sh");
            assert_eq!(for_external_process(p.clone()), p);
        }
    }
}
