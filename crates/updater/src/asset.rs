//! 在当前平台上该安装哪个 release 产物。

use crate::github::{Asset, Release};

/// 与产物一并发布的校验清单文件名。
pub const SUMS: &str = "SHA256SUMS.txt";

/// release 产物名里用的平台标识：Rust target triple（`x86_64-pc-windows-msvc`）。
///
/// 用 triple 而不是自造简称，是为了让产物名、`--target`、CI 的 `RUST_TARGET`
/// 与外部消费方（`prepare.ts` / sidecar tools.lock）指的是同一个名字。
/// 目前只发布 `x86_64-pc-windows-msvc` 产物。其余标识是给未来目标预先约定的命名，
/// 这样新增平台时选择器不用改。
pub fn platform_slug() -> Option<&'static str> {
    Some(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "x86_64-pc-windows-msvc",
        ("windows", "aarch64") => "aarch64-pc-windows-msvc",
        ("macos", "x86_64") => "x86_64-apple-darwin",
        ("macos", "aarch64") => "aarch64-apple-darwin",
        ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
        ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
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

/// release 工作流产出的常规压缩包名，如 `corex-v6.0.2-x86_64-pc-windows-msvc.zip`。
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

/// 改名前的平台标识（`windows-x64` → `x86_64-pc-windows-msvc`，2026-10-12）。
///
/// v2.0.4 – v6.0.1 那一段 release **没有**裸 `corex.exe`，取件只能靠归档名 —— 这正是
/// 归档名回退存在的理由（`corex update --version <旧版本>`），所以旧名要一直留在候选里。
/// 只有 Windows x64 用过它；别的 target 不给这条回退，免得跨平台挑到 Windows 的包。
fn legacy_slug(slug: &str) -> Option<&'static str> {
    (slug == "x86_64-pc-windows-msvc").then_some("windows-x64")
}

/// 按优先级依次尝试的产物名。
///
/// release 工作流会同时发布裸 `corex.exe` *和* 打包压缩文件。
/// 裸二进制优先，因为它不需要解包；压缩文件是它出现之前的 release 的兼容选项，
/// `corex update --version <旧版本>` 能工作就靠这个。选择是数据，
/// 所以优先级在这里重排，而不是在调用方。
fn candidates(tag: &str, slug: &str) -> Vec<(String, Kind)> {
    let mut names = vec![
        (binary_name().to_string(), Kind::Binary),
        (archive_name(tag, slug), Kind::Zip),
    ];
    if let Some(legacy) = legacy_slug(slug) {
        names.push((archive_name(tag, legacy), Kind::Zip));
    }
    names
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
        let names = ["corex.exe", "corex-v6.0.2-x86_64-pc-windows-msvc.zip", SUMS];
        let rel = release("v6.0.2", &names);
        let choice = pick(&rel, "x86_64-pc-windows-msvc").expect("asset");
        assert_eq!(choice.asset.name, binary_name());
        assert_eq!(choice.kind, Kind::Binary);
    }

    #[test]
    fn falls_back_to_archive() {
        let names = ["corex-v6.0.2-x86_64-pc-windows-msvc.zip", SUMS];
        let rel = release("v6.0.2", &names);
        let choice = pick(&rel, "x86_64-pc-windows-msvc").expect("asset");
        assert_eq!(choice.asset.name, "corex-v6.0.2-x86_64-pc-windows-msvc.zip");
        assert_eq!(choice.kind, Kind::Zip);
    }

    #[test]
    fn archive_name_matches_workflow_convention() {
        assert_eq!(
            archive_name("v6.0.1", "x86_64-pc-windows-msvc"),
            "corex-v6.0.1-x86_64-pc-windows-msvc.zip"
        );
    }

    #[test]
    fn no_asset_when_platform_is_unpublished() {
        let rel = release("v6.0.2", &["corex-v6.0.2-aarch64-unknown-linux-gnu.zip"]);
        assert!(pick(&rel, "x86_64-pc-windows-msvc").is_none());
    }

    #[test]
    fn still_finds_pre_rename_archives() {
        // v2.0.4 – v6.0.1 只发了归档，而且用的是改名前的 windows-x64。
        let names = ["corex-v5.3.1-windows-x64.zip", SUMS];
        let rel = release("v5.3.1", &names);
        let choice = pick(&rel, "x86_64-pc-windows-msvc").expect("asset");
        assert_eq!(choice.asset.name, "corex-v5.3.1-windows-x64.zip");
        assert_eq!(choice.kind, Kind::Zip);
    }

    #[test]
    fn new_name_beats_the_legacy_one() {
        let names = [
            "corex-v6.1.0-x86_64-pc-windows-msvc.zip",
            "corex-v6.1.0-windows-x64.zip",
        ];
        let rel = release("v6.1.0", &names);
        let choice = pick(&rel, "x86_64-pc-windows-msvc").expect("asset");
        assert_eq!(choice.asset.name, "corex-v6.1.0-x86_64-pc-windows-msvc.zip");
    }

    #[test]
    fn legacy_fallback_is_windows_only() {
        let rel = release("v5.3.1", &["corex-v5.3.1-windows-x64.zip"]);
        assert!(pick(&rel, "x86_64-unknown-linux-gnu").is_none());
    }
}
