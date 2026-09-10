//! 更新会做什么，在下载任何东西之前就定下来。
//!
//! 计划、调用方选项与结果住在一起，因为它们属于同一段对话：
//! `--check` 停在计划上，确认提示展示的是计划，
//! 而每个结果要么带着计划，要么意味着无事可做。

use semver::Version;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::asset::{Kind, daemon_name};
use crate::error::Error;
use crate::github::{Asset, Release};
use crate::verify::sha256_file;

/// 更新会做什么，在任何东西改变之前展示给用户。
#[derive(Debug, Clone)]
pub struct Plan {
    /// 当前已安装的版本。
    pub current: Version,
    /// 将要安装的版本。
    pub target: Version,
    /// release tag，如 `v6.0.2`。
    pub tag: String,
    /// 面向人的 release 页面。
    pub html_url: String,
    /// 为本平台选中的产物。
    pub asset: Asset,
    /// 二进制如何从 `asset` 中取出。
    pub kind: Kind,
    /// 将被替换的可执行文件。
    pub install_path: PathBuf,
    /// 该 release 附带的 `corex-daemon` 是否与本地的不同。
    pub daemon_outdated: bool,
    /// `target` 是否比 `current` 更旧（故意降级）。
    pub downgrade: bool,
    /// 存放 `install_path` 的目录。
    pub(crate) install_dir: PathBuf,
    /// 来自 release 的 `SHA256SUMS.txt` 的校验和（若有）。
    pub(crate) sums: Option<HashMap<String, String>>,
    /// release 为 `asset` 发布的全部摘要，带来源标注。
    pub(crate) expected: Vec<(String, String)>,
}

impl Plan {
    /// 确认提示用的一行摘要。
    pub fn summary(&self) -> String {
        let verb = if self.downgrade { "降级" } else { "升级" };
        format!(
            "将{verb} corex {} → {}（{}，{}）",
            self.current,
            self.target,
            self.tag,
            human_size(self.asset.size)
        )
    }

    /// 替换成功后需要告诉用户的提示（若有）。
    pub fn notes(&self) -> Vec<String> {
        let mut notes = Vec::new();
        if self.daemon_outdated {
            notes.push(format!(
                "{} 也已更新：请重启守护进程（corex daemon stop && corex daemon start）",
                daemon_name()
            ));
        }
        notes
    }
}

/// [`crate::Updater::check`] 与 [`crate::Updater::run`] 的调用方选项。
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// 即使通道头部与当前版本相同也重装。
    pub force: bool,
    /// 安装指定版本，而不取通道头部版本。
    pub target: Option<String>,
    /// 只下载并校验，不动已安装的可执行文件。
    pub dry_run: bool,
}

/// [`crate::Updater::run`] 的结果。
#[derive(Debug, Clone)]
pub enum RunResult {
    /// 通道头部已经装好了；什么都没下载。
    UpToDate {
        /// 已安装的那个版本。
        current: Version,
    },
    /// 调用方在确认提示上拒绝了。
    Declined(Plan),
    /// `--dry-run`：全部校验完成，只是跳过了替换。
    DryRun(Plan),
    /// 磁盘上的可执行文件已被替换。
    Installed(Plan),
}

impl RunResult {
    /// 计划（当初确实考虑过更新时）。
    pub fn plan(&self) -> Option<&Plan> {
        match self {
            Self::UpToDate { .. } => None,
            Self::Declined(plan) | Self::DryRun(plan) | Self::Installed(plan) => Some(plan),
        }
    }

    /// 磁盘上的可执行文件是否真的被替换了。
    pub fn is_installed(&self) -> bool {
        matches!(self, Self::Installed(_))
    }
}

/// 该 release 附带的 `corex-daemon` 是否与本地不同。
pub(crate) fn daemon_outdated(
    checksums: Option<&HashMap<String, String>>,
    install_dir: &Path,
) -> bool {
    let Some(expected) = checksums.and_then(|sums| sums.get(daemon_name())) else {
        return false;
    };
    let local = install_dir.join(daemon_name());
    if !local.is_file() {
        return false;
    }
    match sha256_file(&local) {
        Ok(actual) => !actual.eq_ignore_ascii_case(expected),
        Err(err) => {
            tracing::debug!(path = %local.display(), error = %err, "读取本地 daemon 哈希失败");
            false
        }
    }
}

/// 目标平台没有已发布的命名方案时的错误。
pub(crate) fn unsupported_platform() -> Error {
    Error::UnsupportedPlatform {
        os: std::env::consts::OS.to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

/// 该 release 里没有任何可安装内容时的错误。
pub(crate) fn no_asset(release: &Release, slug: &str) -> Error {
    Error::NoAsset {
        tag: release.tag.clone(),
        platform: slug.to_string(),
        available: release
            .assets
            .iter()
            .map(|a| a.name.as_str())
            .collect::<Vec<_>>()
            .join(", "),
    }
}

/// 状态行用的人类可读字节数。
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::verify::sha256_hex;

    #[test]
    fn sizes_read_like_a_human_wrote_them() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KiB");
        assert_eq!(human_size(18_658_295), "17.8 MiB");
    }

    #[test]
    fn daemon_check_is_quiet_without_a_manifest_entry() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!daemon_outdated(None, dir.path()));
        let sums = HashMap::from([("corex.exe".to_string(), "a".repeat(64))]);
        assert!(!daemon_outdated(Some(&sums), dir.path()));
    }

    #[test]
    fn daemon_check_detects_a_differing_local_binary() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(daemon_name()), b"old").unwrap();
        let sums = HashMap::from([(daemon_name().to_string(), "a".repeat(64))]);
        assert!(daemon_outdated(Some(&sums), dir.path()));

        let actual = sha256_hex(b"old");
        let sums = HashMap::from([(daemon_name().to_string(), actual)]);
        assert!(!daemon_outdated(Some(&sums), dir.path()));
    }
}
