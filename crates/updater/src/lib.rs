//! Corex 自更新：发现 release、校验校验和、
//! 并原子替换运行中的可执行文件。
//!
//! 本库从不碰终端。调用方传入 [`Reporter`] 来处理
//! 确认提示、下载进度与状态行，这使流程可测，
//! 也让 `--json` / `--yes` 在没有 TTY 时照样能用。
//!
//! ```no_run
//! # async fn demo() -> corex_updater::Result<()> {
//! use corex_updater::{Options, Silent, Updater};
//!
//! let updater = Updater::from_config(Default::default())?;
//! let outcome = updater.run(&Options::default(), &Silent).await?;
//! println!("已安装新版本: {}", outcome.is_installed());
//! # Ok(())
//! # }
//! ```
//!
//! See `docs/reference/自更新.md` for the operating model and `[update]` keys.

mod asset;
pub mod check;
mod digest;
mod error;
mod github;
mod install;
mod plan;
mod proxy;
mod stage;
mod verify;

use corex_core::{UpdateConfig, VERSION};
use semver::Version;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use plan::{daemon_outdated, no_asset, unsupported_platform};
use stage::{binary, smoke_test};

pub use asset::{Choice, Kind, SUMS, archive_name, binary_name, daemon_name, platform_slug};
pub use check::{
    Notice, NotifierEnv, STATE_FILE, State, notice_for, notifier_allowed, run_background,
};
pub use error::{Error, Result};
pub use github::{Asset, Client, Lookup, Release, parse_tag};
pub use plan::{Options, Plan, RunResult, human_size};
pub use verify::{is_sha256, parse_sidecar, parse_sums, sha256_file, sha256_hex};

/// 由调用方提供的钩子，使本库保持与 UI 无关。
pub trait Reporter: Send + Sync {
    /// 只在任何下载或写入发生之前调用一次。
    ///
    /// 返回 `false` 则中止，什么都不变。
    fn confirm(&self, plan: &Plan) -> bool {
        let _ = plan;
        true
    }

    /// 下载进度；服务器不给长度时 `total` 为 `None`。
    fn progress(&self, downloaded: u64, total: Option<u64>) {
        let _ = (downloaded, total);
    }

    /// 自由格式的状态行。
    fn info(&self, message: &str) {
        let _ = message;
    }
}

/// 保持静默、对一切都点头的报告者。
#[derive(Debug, Clone, Copy, Default)]
pub struct Silent;

impl Reporter for Silent {}

/// 自更新驱动器，绑定一份 [`UpdateConfig`] 与一个已安装版本。
#[derive(Debug)]
pub struct Updater {
    config: UpdateConfig,
    current: Version,
    github: Client,
}

impl Updater {
    /// 从 `[update]` 构造，已安装版本取自本二进制。
    pub fn from_config(config: UpdateConfig) -> Result<Self> {
        Self::new(config, VERSION)
    }

    /// 用显式的“当前版本”构造（测试用）。
    pub fn new(config: UpdateConfig, current: &str) -> Result<Self> {
        let github = Client::new(&config)?;
        Ok(Self {
            config,
            current: parse_version(current)?,
            github,
        })
    }

    /// 生效配置。
    pub fn config(&self) -> &UpdateConfig {
        &self.config
    }

    /// 视为已安装的版本。
    pub fn current(&self) -> &Version {
        &self.current
    }

    /// 配置的通道指向的 release，不碰磁盘。
    ///
    /// `etag` 来自 [`State::etag`]，因此重复检查可以直接从缓存得到答案。
    pub async fn latest(&self, etag: Option<&str>) -> Result<Lookup> {
        if !self.config.enabled {
            return Err(Error::Disabled);
        }
        self.github.head(self.config.channel, etag).await
    }

    /// 描述可用的更新（若有）。
    ///
    /// `--check` 用它：解析 release、比较版本，但刻意停在那里，
    /// 于是一次干检查只花掉一次 API 请求。
    pub async fn check(&self, opts: &Options) -> Result<Option<Plan>> {
        self.resolve(opts, false).await
    }

    /// 下载、校验，并（除非 `dry_run`）替换已安装的可执行文件，
    /// 然后对结果做冒烟测试。
    pub async fn run(&self, opts: &Options, reporter: &dyn Reporter) -> Result<RunResult> {
        let Some(plan) = self.resolve(opts, true).await? else {
            return Ok(RunResult::UpToDate {
                current: self.current.clone(),
            });
        };
        if !reporter.confirm(&plan) {
            return Ok(RunResult::Declined(plan));
        }
        // 目标目录只读就提前失败，别先下载。
        install::ensure_writable(&plan.install_dir)?;

        reporter.info(&format!(
            "下载 {}（{}）",
            plan.asset.name,
            human_size(plan.asset.size)
        ));
        let bytes = self
            .github
            .download(&plan.asset.browser_download_url, &mut |done, total| {
                reporter.progress(done, total);
            })
            .await?;
        reporter.progress(bytes.len() as u64, Some(bytes.len() as u64));

        // 任何东西落盘之前，所有已发布的摘要必须一致。
        if plan.expected.is_empty() {
            return Err(Error::ChecksumUnavailable(plan.asset.name.clone()));
        }
        let actual = sha256_hex(&bytes);
        for (source, expected) in &plan.expected {
            verify::require_digest(source, expected, &actual)?;
        }

        let staging = install::stage(&plan.install_dir)?;
        let staged = binary(&plan, staging.path(), &bytes)?;
        smoke_test(&staged, &plan.target)?;

        if opts.dry_run {
            return Ok(RunResult::DryRun(plan));
        }
        install::replace_running_exe(&staged, &plan.install_path)?;
        Ok(RunResult::Installed(plan))
    }

    /// [`Self::check`] 与 [`Self::run`] 共用的前半段。
    ///
    /// `full` 会额外收集校验材料和 daemon 对比，
    /// 两者都要多花请求；`--check` 会跳过它们。
    async fn resolve(&self, opts: &Options, full: bool) -> Result<Option<Plan>> {
        if !self.config.enabled {
            return Err(Error::Disabled);
        }
        let pinned = opts.target.is_some();
        let release = match &opts.target {
            Some(want) => self.github.by_version(&parse_version(want)?).await?,
            None => match self.github.head(self.config.channel, None).await? {
                Lookup::Found { release, .. } => *release,
                // 只在调用方自带 ETag 时才会走到。
                Lookup::NotModified => return Ok(None),
            },
        };
        if release.version <= self.current && !opts.force && !pinned {
            return Ok(None);
        }

        let slug = platform_slug().ok_or_else(unsupported_platform)?;
        let Choice { asset, kind } =
            asset::pick(&release, slug).ok_or_else(|| no_asset(&release, slug))?;
        let asset = asset.clone();

        let install_path = install::current_exe()?;
        let install_dir = install_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let (sums, expected) = if full {
            let sums = self.checksums(&release).await;
            let expected = self.digests(&release, &asset, sums.as_ref()).await;
            (sums, expected)
        } else {
            (None, Vec::new())
        };

        Ok(Some(Plan {
            current: self.current.clone(),
            downgrade: release.version < self.current,
            target: release.version.clone(),
            tag: release.tag.clone(),
            html_url: release.html_url.clone(),
            daemon_outdated: daemon_outdated(sums.as_ref(), &install_dir),
            asset,
            kind,
            install_path,
            install_dir,
            sums,
            expected,
        }))
    }

    /// 与 release 产物一并发布的校验和。
    ///
    /// 尽力而为：清单缺失或格式不对，只是少了一个校验来源，
    /// 它自己从不阻断更新。
    async fn checksums(&self, release: &Release) -> Option<HashMap<String, String>> {
        let asset = release.asset(SUMS)?;
        match self.github.fetch_text(&asset.browser_download_url).await {
            Ok(text) => {
                let sums = parse_sums(&text);
                if sums.is_empty() {
                    tracing::debug!(tag = %release.tag, "SHA256SUMS.txt 中没有可用条目");
                    None
                } else {
                    Some(sums)
                }
            }
            Err(err) => {
                tracing::debug!(tag = %release.tag, error = %err, "获取 SHA256SUMS.txt 失败");
                None
            }
        }
    }
}

/// 解析版本字符串，容忍前面的 `v`。
fn parse_version(text: &str) -> Result<Version> {
    let trimmed = text.trim();
    Version::parse(trimmed.strip_prefix('v').unwrap_or(trimmed))
        .map_err(|_| Error::InvalidVersion(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_malformed_current_version() {
        let err = Updater::new(UpdateConfig::default(), "nightly").unwrap_err();
        assert!(matches!(err, Error::InvalidVersion(_)), "{err}");
    }

    #[test]
    fn accepts_a_leading_v_in_the_current_version() {
        let updater = Updater::new(UpdateConfig::default(), "v6.0.1").unwrap();
        assert_eq!(*updater.current(), Version::new(6, 0, 1));
    }

    #[test]
    fn rejects_a_malformed_repository_slug() {
        let config = UpdateConfig {
            repository: "corex".into(),
            ..UpdateConfig::default()
        };
        assert!(matches!(
            Updater::new(config, "6.0.1").unwrap_err(),
            Error::InvalidRepository(_)
        ));
    }
}
