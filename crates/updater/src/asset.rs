//! 在当前平台上该安装哪个 release 产物。

use crate::github::{Asset, Release};

/// 与产物一并发布的校验清单文件名。
pub const SUMS: &str = "SHA256SUMS.txt";

/// release 产物名里用的平台标识（`windows-x64`）。
///
/// 目前只发布 `windows-x64` 产物。其余标识是给未来目标预先约定的命名，
/// 这样新增平台时选择器不用改。
pub fn platform_slug() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "windows-x64",
        ("windows", "aarch64") => "windows-arm64",
        ("macos", "x86_64") => "macos-x64",
        ("macos", "aarch64") => "macos-arm64",
        ("linux", "x86_64") => "linux-x64",
        ("linux", "aarch64") => "linux-arm64",
        _ => return None,
    })
}

/// 本平台上 CLI 二进制的文件名。
pub fn binary_name() -> &'static str {
    if cfg!(windows) { "corex.exe" } else { "corex" }
}

/// 与 CLI 一同发布的 IPC 守护进程文件名。
pub fn daemon_name() -> &'static str {
    if cfg!(windows) {
        "corex-daemon.exe"
    } else {
        "corex-daemon"
    }
}

/// release 工作流产出的常规压缩包名，如 `corex-v6.0.2-windows-x64.zip`。
pub fn archive_name(tag: &str, slug: &str) -> String {
    format!("corex-{tag}-{slug}.zip")
}

/// 二进制从选中产物里怎么取出来。
///
/// 每种 Kind 都是一套自包含的获取策略；新增一种（`.7z` 包、`.msi`）
/// 就是加一个变体加一条 `binary` 分支，而选择逻辑仍是一张查表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 产物本身就是二进制。
    Binary,
    /// 产物是一个 `.zip`，里面是二进制及其旁车文件。
    Zip,
}

impl Kind {
    /// 日志与 `--json` 输出用的稳定标识。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Zip => "zip",
        }
    }
}

/// 要安装的产物及其解包策略。
#[derive(Debug, Clone, Copy)]
pub struct Choice<'a> {
    /// 要下载的 release 产物。
    pub asset: &'a Asset,
    /// 从中取出二进制的方式。
    pub kind: Kind,
}

/// 按优先级依次尝试的产物名。
///
/// release 工作流会同时发布裸 `corex.exe` *和* 打包压缩文件。
/// 裸二进制优先，因为它不需要解包；压缩文件是它出现之前的 release 的兼容选项，
/// `corex update --version <旧版本>` 能工作就靠这个。选择是数据，
/// 所以优先级在这里重排，而不是在调用方。
fn candidates(tag: &str, slug: &str) -> [(String, Kind); 2] {
    [
        (binary_name().to_string(), Kind::Binary),
        (archive_name(tag, slug), Kind::Zip),
    ]
}

/// 从 `release` 中选出优先级最高的产物。
pub fn pick<'a>(release: &'a Release, slug: &str) -> Option<Choice<'a>> {
    candidates(&release.tag, slug)
        .into_iter()
        .find_map(|(name, kind)| release.asset(&name).map(|asset| Choice { asset, kind }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::github::Asset;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            size: 1,
            digest: None,
            browser_download_url: format!("https://example.invalid/{name}"),
            content_type: None,
        }
    }

    fn release(tag: &str, names: &[&str]) -> Release {
        Release {
            tag: tag.to_string(),
            version: crate::github::parse_tag(tag).expect("valid tag"),
            html_url: String::new(),
            assets: names.iter().map(|n| asset(n)).collect(),
        }
    }

    #[test]
    fn prefers_bare_binary_over_archive() {
        let names = ["corex.exe", "corex-v6.0.2-windows-x64.zip", SUMS];
        let rel = release("v6.0.2", &names);
        let choice = pick(&rel, "windows-x64").expect("asset");
        assert_eq!(choice.asset.name, binary_name());
        assert_eq!(choice.kind, Kind::Binary);
    }

    #[test]
    fn falls_back_to_archive() {
        let names = ["corex-v6.0.2-windows-x64.zip", SUMS];
        let rel = release("v6.0.2", &names);
        let choice = pick(&rel, "windows-x64").expect("asset");
        assert_eq!(choice.asset.name, "corex-v6.0.2-windows-x64.zip");
        assert_eq!(choice.kind, Kind::Zip);
    }

    #[test]
    fn archive_name_matches_workflow_convention() {
        assert_eq!(
            archive_name("v6.0.1", "windows-x64"),
            "corex-v6.0.1-windows-x64.zip"
        );
    }

    #[test]
    fn no_asset_when_platform_is_unpublished() {
        let rel = release("v6.0.2", &["corex-v6.0.2-linux-x64.zip"]);
        assert!(pick(&rel, "windows-x64").is_none());
    }
}
