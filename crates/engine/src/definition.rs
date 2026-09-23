//! 指令 YAML 定义。

use corex_core::{Bucket, Value};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

/// 顶层指令文档。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Directive {
    pub name: String,
    /// 分类，取 [`Bucket`] 的写法（与 `corex actions` 的分组同一套）；缺省为未分类。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "schema", schemars(with = "Option<BucketName>"))]
    pub bucket: Option<Bucket>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputDecl>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    #[cfg_attr(
        feature = "schema",
        schemars(with = "std::collections::HashMap<String, serde_json::Value>")
    )]
    pub variables: HashMap<String, Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    #[serde(default, skip_serializing_if = "Permissions::is_unrestricted")]
    pub permissions: Permissions,
    pub steps: Vec<Step>,
    #[serde(default, skip_serializing_if = "is_abort")]
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

    /// 写回 YAML 文本。
    ///
    /// 只覆盖模型里的字段：**注释与键序不会保留**，而模型已能表达默认值的字段
    /// （空 `inputs`、未声明 `permissions` 等）一律省略。要改指令的编辑者拿到的
    /// 是一份规范化的文档，不是原文的副本。
    pub fn to_yaml_str(&self) -> Result<String, corex_core::EngineError> {
        let mut text = serde_yml::to_string(self)
            .map_err(|e| corex_core::EngineError::ParseError(format!("YAML 序列化失败: {e}")))?;
        if !text.ends_with('\n') {
            text.push('\n');
        }
        Ok(text)
    }
}

/// 序列化时判断「这一步没写参数」，好把 `params: null` 省掉。
fn is_null(value: &Value) -> bool {
    matches!(value, Value::Null)
}

/// `bucket` 在 schema 里的形状。
///
/// [`Bucket`] 本身不派生 `JsonSchema`（corex-core 不依赖 schemars），所以这里按它
/// 的写法复述一遍取值，供编辑指令的人补全；名字由 `Bucket::ALL` 生成，不另抄一份。
#[cfg(feature = "schema")]
struct BucketName;

#[cfg(feature = "schema")]
impl schemars::JsonSchema for BucketName {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "Bucket".into()
    }

    fn json_schema(_generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "enum": Bucket::ALL.iter().map(|bucket| bucket.as_str()).collect::<Vec<_>>(),
        })
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
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
    /// 步骤 id。顺序块默认不写：块本身没有进度 / 引用，`{{steps.<id>}}` 指的是里面
    /// 的步骤。带上它是为了让手写的分支「两条线各叫一个名字」，也免得编辑它的工具
    /// 一保存就把名字抹掉（文档示例里带 id）。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub id: String,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ActionStep {
    pub id: String,
    /// 动作 id，如 `shell.run`、`file.write`。
    pub action: String,
    #[serde(default, skip_serializing_if = "is_null")]
    #[cfg_attr(feature = "schema", schemars(with = "serde_json::Value"))]
    pub params: Value,
    /// 把步骤输出存进某个变量名。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub save_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_error: Option<OnError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct IfStep {
    pub id: String,
    #[serde(rename = "if")]
    pub condition: Condition,
    pub then: Vec<Step>,
    #[serde(default, rename = "else", skip_serializing_if = "Vec::is_empty")]
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u64>,
    /// 能解析成数组的表达式，如 `"{{items}}"`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
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

/// `on_error` 的默认值不写进 YAML：`abort` 与省略是同一件事。
fn is_abort(value: &OnError) -> bool {
    *value == OnError::Abort
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

    /// 该声明下执行 `action_id` 缺少的权限类别；齐全则为空。
    ///
    /// 拒绝信息的措辞只在这里落地，[`Self::allows_action`] 与 [`validate_allowed`] 共用，
    /// 聚合多步时不会把「权限不足」这层前缀重复叠上。
    fn missing_for(
        &self,
        store: &dyn corex_core::ActionStore,
        action_id: &str,
    ) -> Vec<&'static str> {
        if self.is_unrestricted() {
            return Vec::new();
        }
        store
            .permissions_of(action_id)
            .iter()
            .filter(|kind| !self.grants(*kind))
            .map(|kind| kind.name())
            .collect()
    }

    /// 检查该声明下是否允许 `action_id`。
    pub fn allows_action(
        &self,
        store: &dyn corex_core::ActionStore,
        action_id: &str,
    ) -> Result<(), corex_core::ActionError> {
        let missing = self.missing_for(store, action_id);
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

/// 按执行顺序访问步骤树里的每个动作步骤，容器嵌套在这里展开。
///
/// 三道门——动作是否注册、运行时权限够不够、声明的权限覆盖没覆盖（企业 `--strict`）
/// ——共用同一条遍历：容器种类只在这一处展开，加一种容器不必改三遍。
fn walk_actions<'a>(steps: &'a [Step], visit: &mut impl FnMut(&'a ActionStep)) {
    for step in steps {
        match step {
            Step::Action(action) => visit(action),
            Step::If(branch) => {
                walk_actions(&branch.then, visit);
                walk_actions(&branch.else_steps, visit);
            }
            Step::Repeat(repeat) => walk_actions(&repeat.steps, visit),
            Step::Parallel(parallel) => walk_actions(&parallel.parallel, visit),
            Step::Steps(block) => walk_actions(&block.steps, visit),
        }
    }
}

/// Union of the permissions declared by every action in a directive.
///
/// This is intentionally based on the action declarations rather than on step
/// ids or buckets, so callers that need to schedule a whole directive use the
/// same source of truth as validation and the action catalog.
pub fn required_permissions(
    store: &dyn corex_core::ActionStore,
    directive: &Directive,
) -> corex_core::PermissionSet {
    let mut permissions = corex_core::PermissionSet::NONE;
    walk_actions(&directive.steps, &mut |step| {
        permissions = permissions.union(store.permissions_of(&step.action));
    });
    permissions
}

/// 步骤树里若引用了 store 没有的动作就报错。
///
/// 用 [`corex_core::EngineError::ActionNotRegistered`] 而不是一句通用失败：它表示
/// 「调用方给的东西不对」（CLI 退出码 2、IPC 400），不是运行期出错。
pub fn validate_registered(
    store: &dyn corex_core::ActionStore,
    directive: &Directive,
) -> Result<(), corex_core::EngineError> {
    let mut missing = Vec::new();
    walk_actions(&directive.steps, &mut |step| {
        if store.find_action(&step.action).is_none() {
            missing.push(format!("{} ({})", step.action, step.id));
        }
    });
    if missing.is_empty() {
        Ok(())
    } else {
        Err(corex_core::EngineError::ActionNotRegistered(
            missing.join(", "),
        ))
    }
}

/// 步骤树里若有运行时会因权限声明不足而被拒的动作就报错。
///
/// 查的是**运行时那道门**（[`Permissions::allows_action`]），不是 [`validate_permissions`]
/// 的企业门禁：后者额外要求「必须声明 permissions」，而真实运行并不要求。
pub fn validate_allowed(
    store: &dyn corex_core::ActionStore,
    directive: &Directive,
) -> Result<(), corex_core::ActionError> {
    let mut denied = Vec::new();
    walk_actions(&directive.steps, &mut |step| {
        let missing = directive.permissions.missing_for(store, &step.action);
        if !missing.is_empty() {
            denied.push(format!(
                "{}: {} 缺少权限 {}",
                step.id,
                step.action,
                missing.join("+")
            ));
        }
    });
    if denied.is_empty() {
        Ok(())
    } else {
        Err(corex_core::ActionError::PermissionDenied(denied.join("；")))
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
    let mut errs = Vec::new();
    walk_actions(&directive.steps, &mut |step| {
        if let Err(e) = directive.permissions.allows_action(store, &step.action) {
            errs.push(format!("{}: {e}", step.id));
        }
    });
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

    /// 覆盖每一种步骤与条件的指令：写回 YAML 再读一遍，模型必须一模一样。
    const EVERYTHING: &str = r#"
name: everything
bucket: network
description: 覆盖全部步骤种类
version: '1.4'
variables:
  base: /tmp/corex
inputs:
  - name: target
    description: 目标
    required: true
triggers:
  - type: cron
    expr: '0 3 * * *'
  - type: watch
    paths:
      - '{{base}}'
    debounce_ms: 500
permissions:
  filesystem: true
  network: true
steps:
  - id: plain
    action: shell.run
    params:
      command: echo hi
    save_to: out
    when: '{{inputs.target}}'
    on_error: continue
    retry: 2
  - id: branch
    if:
      contains: ['{{out}}', hi]
    then:
      - id: inner
        action: template.render
        params:
          template: '{{out}}'
    else:
      - steps:
          - id: nested
            action: template.render
            params:
              template: x
  - id: loop
    repeat:
      each: '{{items}}'
      as: entry
      index: i
    max_concurrency: 4
    steps:
      - id: per-item
        action: shell.run
        params:
          command: echo '{{entry}}'
  - id: fan-out
    parallel:
      - id: left
        action: shell.run
        params:
          command: echo left
      - id: right
        action: shell.run
        params:
          command: echo right
    max_concurrency: 2
  - steps:
      - id: block
        action: template.render
        params:
          template: done
on_error: skip
"#;

    #[test]
    fn yaml_round_trip_keeps_the_model() {
        let first = Directive::from_yaml_str(EVERYTHING).unwrap();
        assert_eq!(first.bucket, Some(Bucket::Network));
        let text = first.to_yaml_str().unwrap();
        let second = Directive::from_yaml_str(&text).unwrap();

        assert_eq!(
            serde_json::to_string(&second).unwrap(),
            serde_json::to_string(&first).unwrap(),
            "写回的 YAML 读出来和原文不同：\n{text}"
        );
        assert_eq!(second.to_yaml_str().unwrap(), text, "第二次写回应当稳定");
    }

    #[test]
    fn yaml_write_omits_what_the_model_already_defaults() {
        let directive = Directive::from_yaml_str("name: bare\nsteps: []\n").unwrap();
        let text = directive.to_yaml_str().unwrap();
        assert_eq!(text, "name: bare\nsteps: []\n");
    }

    #[test]
    fn unknown_bucket_is_rejected() {
        let err = Directive::from_yaml_str("name: x\nbucket: 别的\nsteps: []\n").unwrap_err();
        assert!(err.to_string().contains("expected one of"), "{err}");
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

    /// 门禁要下到容器里去：`repeat` 里的动作不该因为嵌了一层就被漏掉。
    #[test]
    fn validate_registered_reaches_nested_steps() {
        let yaml = r#"
name: nested
steps:
  - id: loop
    repeat:
      count: 2
    steps:
      - id: inner
        action: does.not.exist
"#;
        let directive = Directive::from_yaml_str(yaml).unwrap();
        let err = validate_registered(&store(), &directive).unwrap_err();
        assert!(err.to_string().contains("does.not.exist (inner)"), "{err}");
    }

    #[test]
    fn required_permissions_reaches_nested_steps() {
        let yaml = r#"
name: resources
steps:
  - id: branch
    parallel:
      - id: screenshot
        action: capture.screenshot
      - id: ui
        action: ui.window.list
"#;
        let directive = Directive::from_yaml_str(yaml).unwrap();
        let permissions = required_permissions(&store(), &directive);
        assert!(permissions.contains(corex_core::PermissionKind::Capture));
        assert!(permissions.contains(corex_core::PermissionKind::Filesystem));
        assert!(permissions.contains(corex_core::PermissionKind::Ui));
    }

    /// 运行时那道门只报被拒的步骤，并点名是谁。
    #[test]
    fn validate_allowed_names_the_offending_step() {
        let mixed = r#"
name: mixed
permissions:
  filesystem: true
steps:
  - id: write
    action: file.write
  - id: shell
    action: shell.run
"#;
        let directive = Directive::from_yaml_str(mixed).unwrap();
        let err = validate_allowed(&store(), &directive).unwrap_err();
        let text = err.to_string();
        assert_eq!(text, "权限不足: shell: shell.run 缺少权限 shell", "{text}");
        assert!(!text.contains("write"), "{text}");

        // 没声明 permissions 就是不受限。
        let open = Directive::from_yaml_str("name: open\nsteps: []\n").unwrap();
        assert!(validate_allowed(&store(), &open).is_ok());
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
