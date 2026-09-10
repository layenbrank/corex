//! daemon IPC 与文件动作共用的路径约束辅助。

use std::path::{Component, Path, PathBuf};

/// 路径解析到允许根之外时的错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathConfineError(pub String);

impl std::fmt::Display for PathConfineError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for PathConfineError {}

/// `path` 是否含有 `..` 分量（在规范化之前判断）。
pub fn path_has_traversal(path: &Path) -> bool {
    path.components().any(|c| matches!(c, Component::ParentDir))
}

/// 保证 `path`（拼接相对路径之后）解析在 `root` 之下。
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

/// 保证 `path` 在 `roots` 之一之下。
///
/// `roots` 为空则不做约束（本地 / 开发默认）。
/// 文件不存在时，按字面与各个根逐一比对检查。
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

/// 为展示与 JSON 统一路径分隔符（Windows 上用 `\`）。
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

/// 用平台原生分隔符展示的路径。
pub fn display_path(path: &Path) -> String {
    normalize_separators(path.to_path_buf())
        .display()
        .to_string()
}

/// 去掉 Windows 的 verbatim（`\\?\`）前缀，使路径能被 `cmd` / PowerShell 接受。
///
/// `canonicalize` 常常给出 `\\?\C:\...`，而 `cmd.exe` 会当“找不到路径”拒绍。
/// 非 Windows 或没有该前缀时是空操作。
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

/// 把一个可能不存在的路径约束在 `root` 之下。
fn confine_missing(root: &Path, path: &Path) -> Result<PathBuf, PathConfineError> {
    let root_canon = root
        .canonicalize()
        .map_err(|e| PathConfineError(format!("无法解析根目录 {}: {e}", root.display())))?;
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        root.join(path)
    };
    // 向上回溯，直到找到一个存在的祖先。
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
        let got = confine_in_roots(std::slice::from_ref(&root), &target).unwrap();
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
