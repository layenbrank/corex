//! 指令 YAML 定义。

use corex_core::Value;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 顶层指令文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Directive {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub inputs: Vec<InputDecl>,
    #[serde(default)]
    #[cfg_attr(
        feature = "schema",
        schemars(with = "std::collections::HashMap<String, serde_json::Value>")
    )]
    pub variables: HashMap<String, Value>,
    #[serde(default)]
    pub triggers: Vec<Trigger>,
    #[serde(default)]
    pub permissions: Permissions,
    pub steps: Vec<Step>,
    #[serde(default)]
    pub on_error: OnError,
}

impl Directive {
    pub fn from_yaml_str(s: &str) -> Result<Self, corex_core::EngineError> {
        serde_yml::from_str(s)
            .map_err(|e| corex_core::EngineError::ParseError(format!("YAML 解析失败: {e}")))
    }

    pub fn from_yaml_file(path: &Path) -> Result<Self, corex_core::EngineError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_yaml_str(&text)
    }
}

/// 声明的指令输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InputDecl {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "Option<serde_json::Value>"))]
    pub default: Option<Value>,
}

/// 单个流水线步骤或控制流节点。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Step {
    /// 普通动作调用。
    Action(ActionStep),
    /// 条件分支。
    If(IfStep),
    /// 重复 / 循环。
    Repeat(RepeatStep),
    /// 并行运行子步骤。
    Parallel(ParallelStep),
    /// 顺序块（放最后：它只要求 `steps`，先试它会把 `repeat` / `if` 抢走）。
    Steps(StepsStep),
}

/// 一组按顺序执行的步骤。
///
/// `parallel` 的分支只能放**一个**步骤，而真实流程常常需要「算完摘要再 PATCH」
/// 这种两步一串的分支；把这组步骤收成一个步骤，语法上就装得下了。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StepsStep {
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActionStep {
    pub id: String,
    /// 动作 id，如 `shell.run`、`file.write`。
    pub action: String,
    #[serde(default)]
    #[cfg_attr(feature = "schema", schemars(with = "serde_json::Value"))]
    pub params: Value,
    /// 把步骤输出存进某个变量名。
    #[serde(default)]
    pub save_to: Option<String>,
    #[serde(default)]
    pub when: Option<Condition>,
    #[serde(default)]
    pub on_error: Option<OnError>,
    #[serde(default)]
    pub retry: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct IfStep {
    pub id: String,
    #[serde(rename = "if")]
    pub condition: Condition,
    pub then: Vec<Step>,
    #[serde(default, rename = "else")]
    pub else_steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RepeatStep {
    pub id: String,
    /// 循环本体：`count` 或 `each`，以及循环变量名。
    pub repeat: RepeatSpec,
    /// 并发度：同一时刻最多跑几个元素 / 几轮；省略或 `1` = 串行。
    ///
    /// 与 `repeat` **同级**，一眼看得出这一步是不是并发的。两种跑法**语义不同**，
    /// 不是纯性能开关：
    /// - 串行（默认）：元素共享一份上下文，前一个元素写下的变量后一个看得见，
    ///   适合元素之间有依赖的循环（累加、按序推进）。
    /// - 并发（`> 1`）：每个元素一份上下文副本，跑完按元素顺序合并回来，元素之间
    ///   互不可见，只适合互不依赖的元素。在途资源 ≈ `max_concurrency × 单元素占用`。
    #[serde(default)]
    pub max_concurrency: Option<usize>,
    pub steps: Vec<Step>,
}

/// 循环本体：`count` 与 `each` 必须设其一。
///
/// 跑法是串行还是并发，由外层 [`RepeatStep::max_concurrency`] 决定。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RepeatSpec {
    #[serde(default)]
    pub count: Option<u64>,
    /// 能解析成数组的表达式，如 `"{{items}}"`。
    #[serde(default)]
    pub each: Option<String>,
    /// 当前元素绑到哪个变量名；`count` 时绑的是序号。
    #[serde(default = "init_repeat_item", rename = "as")]
    pub as_var: String,
    /// 当前序号绑到哪个变量名。
    #[serde(default = "init_repeat_index", rename = "index")]
    pub index_var: String,
}

fn init_repeat_index() -> String {
    "index".into()
}

fn init_repeat_item() -> String {
    "item".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ParallelStep {
    pub id: String,
    /// 一组写死的分支；每支拿一份上下文副本并发跑。
    pub parallel: Vec<Step>,
    /// 同时最多跑几个分支；省略用配置里的 `runtime.max_parallel`（默认 8）。
    #[serde(default)]
    pub max_concurrency: Option<usize>,
}

/// `when` / `if` 用的布尔 / 比较条件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum Condition {
    /// 真值表达式字符串，如 `"{{variables.enabled}}"`。
    Expr(String),
    Eq {
        #[cfg_attr(feature = "schema", schemars(with = "[serde_json::Value; 2]"))]
        eq: [Value; 2],
    },
    Ne {
        #[cfg_attr(feature = "schema", schemars(with = "[serde_json::Value; 2]"))]
        ne: [Value; 2],
    },
    Gt {
        #[cfg_attr(feature = "schema", schemars(with = "[serde_json::Value; 2]"))]
        gt: [Value; 2],
    },
    Lt {
        #[cfg_attr(feature = "schema", schemars(with = "[serde_json::Value; 2]"))]
        lt: [Value; 2],
    },
    /// `contains: [haystack, needle]`。
    ///
    /// 数组含元素、字符串含子串、map 含键。元素比较是宽松的：`0` 与 `"0"` 相等
    /// （服务端返回的分片序号是数字还是字符串不由指令决定）。
    Contains {
        #[cfg_attr(feature = "schema", schemars(with = "[serde_json::Value; 2]"))]
        contains: [Value; 2],
    },
    And {
        and: Vec<Condition>,
    },
    Or {
        or: Vec<Condition>,
    },
    Not {
        not: Box<Condition>,
    },
}

/// 步骤失败时的反应方式。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum OnError {
    #[default]
    Abort,
    Continue,
    Skip,
}

/// 指令可能需要的声明权限。
///
/// 当**所有**标志都为 false（YAML 省略或为空）时，该指令被视为
/// 不设限（allow-all），以兼容简单指令的既有行为。
/// 一旦任一标志为 `true`，未声明的类别即被拒绝。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Permissions {
    #[serde(default)]
    pub network: bool,
    #[serde(default)]
    pub filesystem: bool,
    #[serde(default)]
    pub shell: bool,
    #[serde(default)]
    pub clipboard: bool,
    #[serde(default)]
    pub notifications: bool,
    #[serde(default)]
    pub ui: bool,
    #[serde(default)]
    pub capture: bool,
    #[serde(default)]
    pub secret: bool,
}

impl Permissions {
    /// 没有任何类别被显式启用时为 `true` → 允许所有动作。
    pub fn is_unrestricted(&self) -> bool {
        !self.network
            && !self.filesystem
            && !self.shell
            && !self.clipboard
            && !self.notifications
            && !self.ui
            && !self.capture
            && !self.secret
    }

    /// 检查该声明下是否允许 `action_id`。
    pub fn allows_action(
        &self,
        store: &dyn corex_core::ActionStore,
        action_id: &str,
    ) -> Result<(), corex_core::ActionError> {
        if self.is_unrestricted() {
            return Ok(());
        }
        let missing: Vec<_> = store
            .permissions_of(action_id)
            .iter()
            .filter(|kind| !self.grants(*kind))
            .map(|kind| kind.name())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(corex_core::ActionError::PermissionDenied(format!(
                "指令未声明权限 {} 以执行 {action_id}",
                missing.join("+")
            )))
        }
    }

    /// 该声明是否授予 `kind`。
    fn grants(&self, kind: corex_core::PermissionKind) -> bool {
        use corex_core::PermissionKind as Kind;
        match kind {
            Kind::None => true,
            Kind::Network => self.network,
            Kind::Filesystem => self.filesystem,
            Kind::Shell => self.shell,
            Kind::Clipboard => self.clipboard,
            Kind::Notifications => self.notifications,
            Kind::Ui => self.ui,
            Kind::Capture => self.capture,
            Kind::Secret => self.secret,
        }
    }
}

/// 校验声明的权限覆盖了全部动作步骤（企业 `--strict`）。
pub fn validate_permissions(
    store: &dyn corex_core::ActionStore,
    directive: &Directive,
) -> Result<(), String> {
    if directive.permissions.is_unrestricted() {
        return Err("strict: 必须声明 permissions（当前为 unrestricted / allow-all）".into());
    }
    fn walk(
        steps: &[Step],
        perms: &Permissions,
        store: &dyn corex_core::ActionStore,
        errs: &mut Vec<String>,
    ) {
        for step in steps {
            match step {
                Step::Action(a) => {
                    if let Err(e) = perms.allows_action(store, &a.action) {
                        errs.push(format!("{}: {e}", a.id));
                    }
                }
                Step::If(i) => {
                    walk(&i.then, perms, store, errs);
                    walk(&i.else_steps, perms, store, errs);
                }
                Step::Repeat(r) => walk(&r.steps, perms, store, errs),
                Step::Parallel(p) => walk(&p.parallel, perms, store, errs),
                Step::Steps(s) => walk(&s.steps, perms, store, errs),
            }
        }
    }
    let mut errs = Vec::new();
    walk(&directive.steps, &directive.permissions, store, &mut errs);
    if errs.is_empty() {
        Ok(())
    } else {
        Err(errs.join("; "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 用真实的内置声明，使写错的权限要求不会悄悄溜过。
    fn store() -> corex_registry::ActionRegistry {
        let mut registry = corex_registry::ActionRegistry::new();
        registry.register_builtins();
        registry
    }

    #[test]
    fn unrestricted_allows_everything() {
        let p = Permissions::default();
        assert!(p.is_unrestricted());
        assert!(p.allows_action(&store(), "shell.run").is_ok());
        assert!(p.allows_action(&store(), "http.send").is_ok());
        assert!(p.allows_action(&store(), "template.render").is_ok());
    }

    #[test]
    fn filesystem_only_denies_shell_and_network() {
        let p = Permissions {
            filesystem: true,
            ..Permissions::default()
        };
        assert!(!p.is_unrestricted());
        assert!(p.allows_action(&store(), "file.write").is_ok());
        assert!(p.allows_action(&store(), "template.render").is_ok()); // needs nothing
        assert!(p.allows_action(&store(), "shell.run").is_err());
        assert!(p.allows_action(&store(), "http.send").is_err());
    }

    #[test]
    fn shell_true_allows_shell_denies_http() {
        let p = Permissions {
            shell: true,
            ..Permissions::default()
        };
        assert!(p.allows_action(&store(), "shell.run").is_ok());
        assert!(p.allows_action(&store(), "http.send").is_err());
    }

    #[test]
    fn validate_permissions_rejects_unrestricted() {
        let yaml = r#"
name: bare
steps:
  - id: t
    action: template.render
    params:
      template: "x"
"#;
        let s = Directive::from_yaml_str(yaml).unwrap();
        assert!(validate_permissions(&store(), &s).is_err());
    }
}

/// 指令触发器定义（仅自动化来源；手动执行走 `corex run`）。
#[derive(Debug, Clone)]
pub enum Trigger {
    Cron {
        expr: String,
        /// 可选的 IANA / `local` / `utc`；缺省时用 `RuntimeConfig.cron_timezone`。
        timezone: Option<String>,
    },
    Watch(WatchTrigger),
}

/// watch 触发器字段（`type: watch`）。
pub type WatchTrigger = crate::trigger::WatchConfig;
