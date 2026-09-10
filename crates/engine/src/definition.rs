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
    pub repeat: RepeatSpec,
    pub steps: Vec<Step>,
}

/// 循环规格：`count` 与 `each` 必须设其一。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RepeatSpec {
    #[serde(default)]
    pub count: Option<u64>,
    /// 能解析成数组的表达式，如 `"{{items}}"`。
    #[serde(default)]
    pub each: Option<String>,
    #[serde(default = "init_repeat_item", rename = "as")]
    pub as_var: String,
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
    pub parallel: Vec<Step>,
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
