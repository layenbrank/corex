//! 限流的后台版本检查及其落盘状态。

use crate::error::Result;
use chrono::{DateTime, Duration, Utc};
use corex_core::UpdateConfig;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// 状态文件名，相对 Corex 数据目录。
pub const STATE_FILE: &str = "update-check.json";

/// 通知器的持久状态，使检查保持廉价、横幅保持安静。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct State {
    /// 上次完整检查发生的时间（RFC 3339，UTC）。
    #[serde(default)]
    pub checked_at: Option<String>,
    /// 上次 release 响应的 `ETag`；`304` 不消耗 API 配额。
    #[serde(default)]
    pub etag: Option<String>,
    /// 上次检查时通道指向的 tag。
    #[serde(default)]
    pub latest_tag: Option<String>,
    /// 已宣告过的版本，使每个 release 只打一次横幅。
    #[serde(default)]
    pub notified_version: Option<String>,
}

impl State {
    /// 读取状态文件，任何错误都回退到空状态。
    ///
    /// 文件缺失或读不了，只意味着“从未检查过”：
    /// 对全新安装而言这正是正确行为。
    pub fn read(data_dir: &Path) -> Self {
        std::fs::read_to_string(data_dir.join(STATE_FILE))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// 落盘状态。写入失败只记日志，绝不算致命——
    /// 只读的数据目录不该弄坏用户真正要跑的那条命令。
    pub fn save(&self, data_dir: &Path) -> Result<()> {
        let path = data_dir.join(STATE_FILE);
        let text = serde_json::to_string_pretty(self)?;
        if let Err(err) = std::fs::write(&path, text) {
            tracing::debug!(path = %path.display(), error = %err, "写入更新检查状态失败");
        }
        Ok(())
    }

    /// 距上次检查是否已超过配置的间隔。
    pub fn is_stale(&self, interval_hours: u64, now: DateTime<Utc>) -> bool {
        if interval_hours == 0 {
            return true;
        }
        let Some(checked) = self.checked_at.as_deref().and_then(parse_timestamp) else {
            return true;
        };
        now.signed_duration_since(checked) >= Duration::hours(interval_hours as i64)
    }
}

/// 把 RFC 3339 时间戳解析成 UTC。
pub fn parse_timestamp(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|value| value.with_timezone(&Utc))
}

/// 值得告知用户的新版本。
#[derive(Debug, Clone)]
pub struct Notice {
    /// 当前已安装的版本。
    pub current: semver::Version,
    /// 配置通道上可用的更新版本。
    pub latest: semver::Version,
}

impl Notice {
    /// 打到 stderr 的那一行提示。
    pub fn message(&self) -> String {
        format!(
            "corex {} 可用（当前 {}）。运行 `corex update` 升级。",
            self.latest, self.current
        )
    }
}

/// 当 `latest` 严格新于 `current` 时构造提示。
pub fn notice_for(current: &semver::Version, latest: &semver::Version) -> Option<Notice> {
    (latest > current).then(|| Notice {
        current: current.clone(),
        latest: latest.clone(),
    })
}

/// 进程环境，只读一次，使通知器的判断可测试。
#[derive(Debug, Clone, Copy)]
pub struct NotifierEnv {
    /// `COREX_NO_UPDATE_NOTIFIER` 被设为非空值。
    pub opted_out: bool,
    /// `DO_NOT_TRACK` 被设为非空值。
    pub do_not_track: bool,
    /// `CI` 的值为真。
    pub ci: bool,
    /// stdout 或 stderr 是终端。
    pub interactive: bool,
}

impl NotifierEnv {
    /// 快照当前进程环境。
    pub fn from_process() -> Self {
        Self {
            opted_out: env_set("COREX_NO_UPDATE_NOTIFIER"),
            do_not_track: env_set("DO_NOT_TRACK"),
            ci: env_truthy("CI"),
            interactive: is_interactive(),
        }
    }
}

/// 在 `config` 与 `env` 下后台通知器是否允许运行。
///
/// 只要命令不是面向人的就跳过检查：输出被管道接走、跑在 CI 里，或任何显式退出开关。
pub fn notifier_allowed(config: &UpdateConfig, env: &NotifierEnv) -> bool {
    config.enabled
        && config.check_on_start
        && !env.opted_out
        && !env.do_not_track
        && !env.ci
        && env.interactive
}

/// 跑一次限流的后台检查，落盘结果，并在有新版本尚未宣告时返回一条提示。
///
/// 永不失败。通知器不能改变用户真正那条命令的退出码或输出，所以每条错误路径都返回 `None`。
///
/// 间隔未到时复用缓存的 tag，于是已知的升级在限流运行里仍然会浮现，且不多花一次 API 请求。
pub async fn run_background(updater: &crate::Updater, data_dir: &Path) -> Option<Notice> {
    let mut state = State::read(data_dir);

    if state.is_stale(updater.config().check_interval, Utc::now()) {
        match updater.latest(state.etag.as_deref()).await {
            Ok(found) => {
                // `304` 证明缓存的 tag 仍是通道头部，所以这种情况下只值得记录时间戳。
                if let crate::Lookup::Found { release, etag } = found {
                    state.latest_tag = Some(release.tag);
                    state.etag = etag;
                }
                state.checked_at = Some(Utc::now().to_rfc3339());
                let _ = state.save(data_dir);
            }
            Err(err) => {
                tracing::debug!(error = %err, "后台更新检查失败");
                return None;
            }
        }
    }

    // 无论上面哪种结果，缓存的 tag 之后都是权威的，
    // 所以限流运行里已知的升级仍能浮现，不需要再发请求。
    let latest = crate::github::parse_tag(state.latest_tag.as_deref()?)?;
    let notice = notice_for(updater.current(), &latest)?;
    // 每个 release 只宣告一次，横幅不会每次运行都重复。
    if state.notified_version.as_deref() == Some(notice.latest.to_string().as_str()) {
        return None;
    }
    state.notified_version = Some(notice.latest.to_string());
    let _ = state.save(data_dir);
    Some(notice)
}

/// stdout 或 stderr 是否接在终端上。
fn is_interactive() -> bool {
    use std::io::IsTerminal;
    std::io::stdout().is_terminal() || std::io::stderr().is_terminal()
}

/// `name` 是否存在且值非空。
fn env_set(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| !value.trim().is_empty())
}

/// `name` 的值是否可读作“开”。
fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .map(|value| {
            let value = value.trim().to_ascii_lowercase();
            !matches!(value.as_str(), "" | "0" | "false" | "no" | "off")
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use corex_core::UpdateConfig;

    fn interactive() -> NotifierEnv {
        NotifierEnv {
            opted_out: false,
            do_not_track: false,
            ci: false,
            interactive: true,
        }
    }

    #[test]
    fn fresh_state_is_always_stale() {
        let state = State::default();
        assert!(state.is_stale(24, Utc::now()));
    }

    #[test]
    fn interval_is_respected() {
        let now = Utc::now();
        let checked = |hours: i64| State {
            checked_at: Some((now - Duration::hours(hours)).to_rfc3339()),
            ..State::default()
        };
        assert!(!checked(23).is_stale(24, now));
        assert!(checked(25).is_stale(24, now));
        // 间隔为 0 意味着“每次运行都检查”。
        assert!(checked(1).is_stale(0, now));
    }

    #[test]
    fn unparsable_timestamp_is_treated_as_stale() {
        let state = State {
            checked_at: Some("yesterday".into()),
            ..State::default()
        };
        assert!(state.is_stale(24, Utc::now()));
    }

    #[test]
    fn state_round_trips_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let state = State {
            checked_at: Some(Utc::now().to_rfc3339()),
            etag: Some("\"abc\"".into()),
            latest_tag: Some("v6.0.2".into()),
            notified_version: Some("6.0.2".into()),
        };
        state.save(dir.path()).unwrap();
        let reread = State::read(dir.path());
        assert_eq!(reread.etag, state.etag);
        assert_eq!(reread.latest_tag, state.latest_tag);
        assert_eq!(reread.notified_version, state.notified_version);
    }

    #[test]
    fn missing_state_file_is_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(State::read(&dir.path().join("absent")).etag.is_none());
    }

    #[test]
    fn notifier_respects_every_opt_out() {
        let config = UpdateConfig::default();
        assert!(notifier_allowed(&config, &interactive()));

        let mut disabled = config.clone();
        disabled.enabled = false;
        assert!(!notifier_allowed(&disabled, &interactive()));

        let mut no_check = config.clone();
        no_check.check_on_start = false;
        assert!(!notifier_allowed(&no_check, &interactive()));

        let mut opted_out = interactive();
        opted_out.opted_out = true;
        assert!(!notifier_allowed(&config, &opted_out));

        let mut dnt = interactive();
        dnt.do_not_track = true;
        assert!(!notifier_allowed(&config, &dnt));

        let mut ci = interactive();
        ci.ci = true;
        assert!(!notifier_allowed(&config, &ci));

        let mut piped = interactive();
        piped.interactive = false;
        assert!(!notifier_allowed(&config, &piped));
    }

    #[test]
    fn notice_only_for_newer_releases() {
        let current = semver::Version::new(6, 0, 1);
        let notice = notice_for(&current, &semver::Version::new(6, 0, 2)).expect("newer");
        assert!(notice.message().contains("6.0.2"));
        assert!(notice.message().contains("corex update"));
        assert!(notice_for(&current, &semver::Version::new(6, 0, 1)).is_none());
        assert!(notice_for(&current, &semver::Version::new(6, 0, 0)).is_none());
    }
}
