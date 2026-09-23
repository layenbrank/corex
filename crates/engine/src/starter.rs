//! 首次使用时写入指令目录的起步指令。
//!
//! 数据目录里一条指令都没有时，宿主与 CLI 看到的是一个空页面——不知道能做什么，
//! 也不知道该写什么。这里在**默认**指令目录为空时写入几条通用指令（有备份、查目录、
//! 摘要、提醒），让第一次启动就有东西可跑、可改。
//!
//! 只在默认目录、只在空目录发生：目录里已经有指令就一条都不写。用户删掉某条不会
//! 被补回来；想让它们回来就把目录腾空——「空目录」本身就是唯一的门槛，没有额外开关。
//! `--dir` / `--directives` 指定的目录一律不碰。

use std::fs;
use std::path::Path;
use tracing::{debug, warn};

/// 内置起步指令：`(文件名主干, YAML)`，文件名主干同时也是指令名
/// （daemon 列表取文件 stem，两者必须一致）。
///
/// 名字刻意避开 `examples/directives`：同名时用户目录里的这份会遮住示例。
const STARTERS: &[(&str, &str)] = &[
    (
        "dir-backup",
        include_str!("../assets/starters/dir-backup.yaml"),
    ),
    ("dir-tree", include_str!("../assets/starters/dir-tree.yaml")),
    ("reminder", include_str!("../assets/starters/reminder.yaml")),
    (
        "system-info",
        include_str!("../assets/starters/system-info.yaml"),
    ),
    (
        "text-fingerprint",
        include_str!("../assets/starters/text-fingerprint.yaml"),
    ),
];

/// 起步指令名。
pub fn names() -> Vec<&'static str> {
    STARTERS.iter().map(|(name, _)| *name).collect()
}

/// 往 `directory` 写入起步指令，返回写了几条。
///
/// 目录里已经有 `*.yaml` / `*.yml`（大小写不敏感）就原样返回 0——那是用户的指令目录，
/// 不是空房子。单个文件写失败只告警：起步指令不值得让调用方启动失败。
pub fn seed(directory: &Path) -> std::io::Result<usize> {
    fs::create_dir_all(directory)?;
    if has_directive(directory)? {
        return Ok(0);
    }
    let mut written = 0;
    for (name, yaml) in STARTERS {
        let path = directory.join(format!("{name}.yaml"));
        match fs::write(&path, yaml) {
            Ok(()) => written += 1,
            Err(e) => warn!(path = %path.display(), error = %e, "起步指令写入失败"),
        }
    }
    debug!(count = written, dir = %directory.display(), "已写入起步指令");
    Ok(written)
}

/// 目录里是否已有指令文件（只看一层）。
fn has_directive(directory: &Path) -> std::io::Result<bool> {
    for entry in fs::read_dir(directory)? {
        let extension = entry?
            .path()
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase());
        if matches!(extension.as_deref(), Some("yaml") | Some("yml")) {
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn yaml_files(dir: &Path) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("yaml"))
            .collect();
        found.sort();
        found
    }

    #[test]
    fn seeds_empty_directory() {
        let dir = tempfile::tempdir().unwrap();
        let written = seed(dir.path()).unwrap();
        assert_eq!(written, STARTERS.len());
        assert_eq!(yaml_files(dir.path()).len(), STARTERS.len());
        for (name, yaml) in STARTERS {
            let path = dir.path().join(format!("{name}.yaml"));
            assert_eq!(&fs::read_to_string(&path).unwrap(), yaml);
        }
    }

    /// 目录是用户的，不是我们的：已经放过东西就一条都不写，更不覆盖。
    #[test]
    fn keeps_directory_with_directives() {
        let dir = tempfile::tempdir().unwrap();
        let mine = dir.path().join("mine.yaml");
        fs::write(&mine, "name: mine\nsteps: []\n").unwrap();

        assert_eq!(seed(dir.path()).unwrap(), 0);
        assert_eq!(yaml_files(dir.path()), vec![mine.clone()]);
        assert_eq!(
            fs::read_to_string(&mine).unwrap(),
            "name: mine\nsteps: []\n"
        );
    }

    /// 删掉起步指令就是不想要了：再跑一次也不会长回来。
    #[test]
    fn removed_starters_stay_removed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(seed(dir.path()).unwrap() > 0);
        fs::remove_file(dir.path().join("dir-tree.yaml")).unwrap();

        assert_eq!(seed(dir.path()).unwrap(), 0);
        assert!(!dir.path().join("dir-tree.yaml").exists());
    }

    /// README、编辑器备份之类的杂物不算「已经放过东西」，空目录该播种还是播种。
    #[test]
    fn other_files_do_not_block_seeding() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("README.md"), "笔记").unwrap();
        fs::write(dir.path().join("hello.yaml.bak"), "x").unwrap();

        assert_eq!(seed(dir.path()).unwrap(), STARTERS.len());
    }

    #[test]
    fn names_match_files() {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path()).unwrap();
        let mut expected: Vec<String> = names().iter().map(|n| format!("{n}.yaml")).collect();
        expected.sort();
        let actual: Vec<String> = yaml_files(dir.path())
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(actual, expected);
    }
}
