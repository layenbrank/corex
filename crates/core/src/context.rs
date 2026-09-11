//! 传给每次动作调用的执行上下文。

use crate::progress::{Mark, Observer, Owned, Spot, Unit};
use crate::value::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

/// 基线 UI 预设名（`[runtime].ui_profile`）。
pub const UI_PROFILE: &str = "baseline";

/// 省略 `[runtime].max_parallel` 时的最大并行步骤数。
pub const MAX_PARALLEL: usize = 8;

/// `ui.element.*` 的基线 `selectors[]` 链长度上限。
pub const SELECTOR_DEPTH: usize = 8;

/// 从 `config/corex.toml` 读入（外加覆盖）的运行时开关。
pub const RUNTIME_CONFIG: &str = "config/corex.toml";

/// 工作区包版本，编译期取得。
///
/// 所有工作区 crate 都继承 `[workspace.package].version`，所以它等于
/// 发布的 tag（去掉开头的 `v`）。`corex update` 与 daemon 握手都用它。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 配置里的运行时插件 / 动作启用开关。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PluginConfig {
    /// 插件目录（绝对路径，或相对数据目录）。
    #[serde(default = "init_plugin_dir")]
    pub plugin_dir: PathBuf,
    /// 按 id 停用整个插件。
    #[serde(default)]
    pub disabled: Vec<String>,
    /// 停用单个动作 id（如 `shell.run`）。
    #[serde(default)]
    pub disabled_actions: Vec<String>,
}

fn init_plugin_dir() -> PathBuf {
    PathBuf::from("plugins")
}

/// 只追加的执行历史设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryConfig {
    /// 为真时把流水线执行记录到 JSONL。
    #[serde(default = "init_history_enabled")]
    pub enabled: bool,
    /// 文件名，或相对数据目录的路径。
    #[serde(default = "init_history_file")]
    pub file: PathBuf,
}

fn init_history_enabled() -> bool {
    true
}

fn init_history_file() -> PathBuf {
    PathBuf::from("history.jsonl")
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: init_history_enabled(),
            file: init_history_file(),
        }
    }
}

/// 配置里 `[daemon]` 的 IPC / 锁设置。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DaemonConfig {
    /// Unix socket 路径（相对数据目录）或 Windows 命名管道路径。
    #[serde(default)]
    pub socket_path: Option<PathBuf>,
    /// 单实例锁文件（非绝对路径时相对数据目录）。
    #[serde(default)]
    pub lock_path: Option<PathBuf>,
    /// IPC 的共享密钥。为空 / 未设 → 自动生成到数据目录的 `token`。
    #[serde(default)]
    pub token: Option<String>,
}

/// 配置里 `[logging]` 的日志设置。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoggingConfig {
    #[serde(default = "init_log_level")]
    pub level: String,
    #[serde(default)]
    pub json: bool,
}

fn init_log_level() -> String {
    "info".into()
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: init_log_level(),
            json: false,
        }
    }
}

/// 来自 `[runtime].ui_profile` 的 UI 自动化预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiProfilePreset {
    pub selector_depth: usize,
    pub settle_limit: u64,
}

impl UiProfilePreset {
    pub fn parse(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "fast" => Self {
                selector_depth: 5,
                settle_limit: 2_000,
            },
            "patient" => Self {
                selector_depth: 12,
                settle_limit: 0,
            },
            // 向后兼容别名
            "default" | "baseline" | "" => Self::baseline(),
            _ => Self::baseline(),
        }
    }

    pub fn baseline() -> Self {
        Self {
            selector_depth: SELECTOR_DEPTH,
            settle_limit: 0,
        }
    }
}

/// `corex update` 跟随的发布通道（`[update].channel`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    /// 既不是草稿也不是预发布的最新 release（`/releases/latest`）。
    #[default]
    Stable,
    /// 最新的 `-alpha.N` 预发布版。
    Alpha,
    /// 最新的 `-beta.N` 预发布版。
    Beta,
    /// 最新的 `-rc.N` 预发布版。
    Rc,
}

impl UpdateChannel {
    /// 全部通道，按晋级顺序。
    pub const ALL: [Self; 4] = [Self::Stable, Self::Alpha, Self::Beta, Self::Rc];

    /// 配置与 `--channel` 里使用的小写标识。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Alpha => "alpha",
            Self::Beta => "beta",
            Self::Rc => "rc",
        }
    }

    /// release tag 里用的预发布标签，stable 为 `None`。
    pub fn prerelease_label(self) -> Option<&'static str> {
        match self {
            Self::Stable => None,
            Self::Alpha => Some("alpha"),
            Self::Beta => Some("beta"),
            Self::Rc => Some("rc"),
        }
    }

    /// 该通道是否跟踪预发布版，而不是只跟稳定版。
    pub fn is_prerelease(self) -> bool {
        self.prerelease_label().is_some()
    }

    /// `tag`（如 `v6.0.2-beta.1`）是否属于该通道。
    pub fn accepts_tag(self, tag: &str) -> bool {
        let version = tag.strip_prefix('v').unwrap_or(tag);
        match self.prerelease_label() {
            None => !version.contains('-'),
            Some(label) => version
                .split_once('-')
                .is_some_and(|(_, pre)| pre == label || pre.starts_with(&format!("{label}."))),
        }
    }
}

impl std::fmt::Display for UpdateChannel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for UpdateChannel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let key = s.trim().to_ascii_lowercase();
        Self::ALL
            .into_iter()
            .find(|c| c.as_str() == key)
            .ok_or_else(|| {
                format!(
                    "未知通道 `{s}`，可选：{}",
                    Self::ALL.map(Self::as_str).join(" | ")
                )
            })
    }
}

/// 配置里 `[update]` 的自更新设置。
///
/// 企业 / 离线部署应设 `enabled = false`，这会同时关掉
/// `corex update` 与后台版本检查。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfig {
    /// 总开关。
    #[serde(default = "init_update_enabled")]
    pub enabled: bool,
    /// CLI 启动时跑一次限流的后台版本检查。
    #[serde(default = "init_update_check_on_start")]
    pub check_on_start: bool,
    /// 两次后台检查之间的最少小时数（0 = 每次都查）。
    #[serde(default = "init_check_interval")]
    pub check_interval: u64,
    /// 要跟随的发布通道。
    #[serde(default)]
    pub channel: UpdateChannel,
    /// GitHub 仓库 slug，`owner/repo`。
    #[serde(default = "init_update_repository")]
    pub repository: String,
    /// 覆盖 GitHub API 根地址（GitHub Enterprise / 内网镜像）。
    #[serde(default)]
    pub api_base_url: Option<String>,
    /// 保存 GitHub token 的环境变量名；有它可把匿名调用 60 次/小时的
    /// API 限额提高。`None` 表示完全不查 token。
    #[serde(default = "init_update_token_env")]
    pub token_env: Option<String>,
    /// 环境里没配代理时，回退到 Windows 系统代理设置。
    ///
    /// `reqwest` 只认 `HTTP_PROXY` / `HTTPS_PROXY` / `NO_PROXY`，但在 Windows 上
    /// 代理通常配在“设置”里，没有这一步，走代理的机器就连不上 release CDN。
    /// 显式设的环境变量永远优先。
    #[serde(default = "init_update_use_system_proxy")]
    pub use_system_proxy: bool,
    /// 替换运行中的可执行文件之前是否询问。
    #[serde(default = "init_update_confirm")]
    pub require_confirmation: bool,
    /// 单个 HTTP 请求的超时秒数。
    #[serde(default = "init_timeout")]
    pub timeout: u64,
}

fn init_update_enabled() -> bool {
    true
}

fn init_update_check_on_start() -> bool {
    true
}

fn init_check_interval() -> u64 {
    24
}

fn init_update_repository() -> String {
    "layenbrank/corex".into()
}

fn init_update_token_env() -> Option<String> {
    Some("COREX_GITHUB_TOKEN".into())
}

fn init_update_use_system_proxy() -> bool {
    true
}

fn init_update_confirm() -> bool {
    true
}

fn init_timeout() -> u64 {
    30
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            enabled: init_update_enabled(),
            check_on_start: init_update_check_on_start(),
            check_interval: init_check_interval(),
            channel: UpdateChannel::default(),
            repository: init_update_repository(),
            api_base_url: None,
            token_env: init_update_token_env(),
            use_system_proxy: init_update_use_system_proxy(),
            require_confirmation: init_update_confirm(),
            timeout: init_timeout(),
        }
    }
}

/// 从 `config/corex.toml` 读入（外加覆盖）的运行时开关。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default)]
    pub plugins: PluginConfig,
    #[serde(default)]
    pub history: HistoryConfig,
    #[serde(default)]
    pub daemon: DaemonConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default = "init_max_parallel")]
    pub max_parallel: usize,
    /// 单步超时秒数（0 = 不限）。
    #[serde(default)]
    pub step_timeout: u64,
    /// 为真时，未声明权限的指令会被拒绍（企业模式）。
    #[serde(default)]
    pub strict_permissions: bool,
    /// file.* 动作允许的文件系统根。为空 = 不做约束（开发模式）。
    #[serde(default)]
    pub filesystem_roots: Vec<PathBuf>,
    /// UI 预设：`baseline` | `fast` | `patient`（见 [`UiProfilePreset`]）。
    #[serde(default = "init_ui_profile")]
    pub ui_profile: String,
    /// `ui.element.*` 的 `selectors[]` 回退链长度上限（0 = 用 `ui_profile` 预设值）。
    #[serde(default)]
    pub ui_selector_depth: usize,
    /// 每次指令运行中 `ui.wait` 固定等待毫秒数的总上限（0 = 不限）。
    #[serde(default)]
    pub ui_settle_limit: u64,
    /// cron 触发器 / `cron.schedule` 的默认时区。
    ///
    /// 接受 `local`（系统本地）、`utc`，或固定偏移（如 `+08:00`）。
    /// 触发器上的 `timezone` 会覆盖它。默认：`local`。
    #[serde(default = "init_cron_timezone")]
    pub cron_timezone: String,
    /// 来自 `[update]` 的自更新设置。见 [`UpdateConfig`]。
    #[serde(default)]
    pub update: UpdateConfig,
}

fn init_ui_profile() -> String {
    UI_PROFILE.into()
}

fn init_max_parallel() -> usize {
    MAX_PARALLEL
}

fn init_cron_timezone() -> String {
    "local".into()
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        let preset = UiProfilePreset::baseline();
        Self {
            plugins: PluginConfig::default(),
            history: HistoryConfig::default(),
            daemon: DaemonConfig::default(),
            logging: LoggingConfig::default(),
            max_parallel: init_max_parallel(),
            step_timeout: 0,
            strict_permissions: false,
            filesystem_roots: Vec::new(),
            ui_profile: init_ui_profile(),
            ui_selector_depth: preset.selector_depth,
            ui_settle_limit: preset.settle_limit,
            cron_timezone: init_cron_timezone(),
            update: UpdateConfig::default(),
        }
    }
}

impl RuntimeConfig {
    /// `selectors[]` 回退链上限：显式配置优先，否则取 `ui_profile` 预设。
    pub fn ui_depth(&self) -> usize {
        if self.ui_selector_depth > 0 {
            return self.ui_selector_depth;
        }
        UiProfilePreset::parse(&self.ui_profile).selector_depth
    }

    /// 应用 `ui_profile` 预设；`overrides` 里的显式覆盖优先。
    pub fn ui_profile(&mut self, profile: &str, overrides: UiProfileOverrides) {
        self.ui_profile = profile.to_string();
        let preset = UiProfilePreset::parse(profile);
        self.ui_selector_depth = overrides.selector_depth.unwrap_or(preset.selector_depth);
        self.ui_settle_limit = overrides.settle_limit.unwrap_or(preset.settle_limit);
    }
}

/// 配置文件里可选的显式 UI 运行时覆盖值。
#[derive(Debug, Clone, Copy, Default)]
pub struct UiProfileOverrides {
    pub selector_depth: Option<usize>,
    pub settle_limit: Option<u64>,
}

/// 一次指令运行内缓存的 UI 自动化范围。
#[derive(Debug, Clone, Default)]
pub struct UiSession {
    pub scope_hwnd: Option<i64>,
    pub scope_title: Option<String>,
    /// `ui.wait` 累积的固定睡眠毫秒数。
    pub settle_ms_used: u64,
}

/// 指令 / 流水线运行期间可用的可变状态。
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// 用户自定义 / 指令声明的变量。
    pub variables: HashMap<String, Value>,
    /// 运行期解析出的指令声明输入。
    pub input: HashMap<String, Value>,
    /// 来自启动器 / 上一条指令的可选负载。
    pub directive_input: Option<Value>,
    /// 已完成步骤的输出，以 step id 为键。
    pub step_outputs: HashMap<String, Value>,
    /// 进程环境快照（字符串值）。
    pub env: HashMap<String, String>,
    /// 运行时配置。
    pub config: RuntimeConfig,
    /// UI 自动化会话（窗口范围、稳定等待预算）。
    pub ui_session: UiSession,
    /// 执行进度的上报口；`None` 时所有上报都是空操作。
    pub observer: Option<Arc<dyn Observer>>,
    /// 当前正在执行的动作步骤，由引擎在调用动作前设置。
    current: Option<Owned>,
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new(RuntimeConfig::default())
    }
}

impl ExecutionContext {
    pub fn new(config: RuntimeConfig) -> Self {
        let env = std::env::vars().collect();
        Self {
            variables: HashMap::new(),
            input: HashMap::new(),
            directive_input: None,
            step_outputs: HashMap::new(),
            env,
            config,
            ui_session: UiSession::default(),
            observer: None,
            current: None,
        }
    }

    pub fn with_input(mut self, input: HashMap<String, Value>) -> Self {
        self.input = input;
        self
    }

    pub fn with_variables(mut self, variables: HashMap<String, Value>) -> Self {
        self.variables = variables;
        self
    }

    pub fn with_directive_input(mut self, value: Value) -> Self {
        self.directive_input = Some(value);
        self
    }

    /// 记录当前动作步骤，供动作上报进度时标注自己。
    ///
    /// 只由引擎在调用动作前后设置；动作不应当调用它。
    pub fn enter_step(&mut self, id: impl Into<String>, action: impl Into<String>) {
        self.current = Some(Owned {
            id: id.into(),
            action: action.into(),
        });
    }

    /// 清除当前动作步骤；只由引擎调用。
    pub fn leave_step(&mut self) {
        self.current = None;
    }

    /// 上报当前步骤的分块进度。
    ///
    /// 没有上报口、或不在动作步骤内时是空操作，因此动作可以无条件调用它。
    /// `unit` 只影响展示，上报口不负责换算。
    pub fn chunk(&self, done: u64, total: Option<u64>, unit: Unit) {
        let (Some(observer), Some(current)) = (&self.observer, &self.current) else {
            return;
        };
        observer.chunk(
            Spot {
                id: &current.id,
                action: &current.action,
            },
            Mark { done, total, unit },
        );
    }

    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    pub fn set_step_output(&mut self, step_id: impl Into<String>, value: Value) {
        self.step_outputs.insert(step_id.into(), value);
    }

    pub fn find_variable(&self, name: &str) -> Option<&Value> {
        self.variables.get(name)
    }

    pub fn set_ui_scope(&mut self, hwnd: i64, title: Option<String>) {
        self.ui_session.scope_hwnd = Some(hwnd);
        self.ui_session.scope_title = title;
    }

    pub fn add_ui_settle_ms(&mut self, ms: u64) -> Result<(), String> {
        let limit = self.config.ui_settle_limit;
        let next = self.ui_session.settle_ms_used.saturating_add(ms);
        if limit > 0 && next > limit {
            return Err(format!(
                "ui.wait 累计 {next}ms 超过 ui_settle_limit={limit}"
            ));
        }
        self.ui_session.settle_ms_used = next;
        Ok(())
    }

    /// `ui.element.*` 的 `selectors[]` 长度上限（来自运行时配置 / 预设）。
    pub fn ui_depth(&self) -> usize {
        self.config.ui_depth()
    }

    /// 归并一个并行分支上下文的输出（以及新写入的变量）。
    ///
    /// 键冲突时一律以分支的 `step_outputs` 为准。`other` 里存在、
    /// 且与 `self` 不同的变量会被复制过来（按调用顺序后写者胜）。
    pub fn merge_from_branch(&mut self, other: &ExecutionContext) {
        for (k, v) in &other.step_outputs {
            self.step_outputs.insert(k.clone(), v.clone());
        }
        for (k, v) in &other.variables {
            match self.variables.get(k) {
                Some(existing) if existing == v => {}
                _ => {
                    self.variables.insert(k.clone(), v.clone());
                }
            }
        }
    }
}

#[cfg(test)]
mod ui_profile_tests {
    use super::*;

    #[test]
    fn baseline_profile_selector_chain_is_8() {
        let cfg = RuntimeConfig::default();
        assert_eq!(cfg.ui_profile, UI_PROFILE);
        assert_eq!(cfg.ui_depth(), SELECTOR_DEPTH);
    }

    #[test]
    fn legacy_default_profile_alias() {
        assert_eq!(
            UiProfilePreset::parse("default").selector_depth,
            SELECTOR_DEPTH
        );
    }

    #[test]
    fn patient_profile_takes_effect() {
        let mut cfg = RuntimeConfig::default();
        cfg.ui_profile("patient", UiProfileOverrides::default());
        assert_eq!(cfg.ui_depth(), 12);
    }

    #[test]
    fn explicit_chain_overrides_profile() {
        let mut cfg = RuntimeConfig::default();
        cfg.ui_profile(
            "fast",
            UiProfileOverrides {
                selector_depth: Some(10),
                settle_limit: None,
            },
        );
        assert_eq!(cfg.ui_depth(), 10);
    }
}
