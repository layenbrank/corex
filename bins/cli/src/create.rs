//! `corex create`：生成一条能跑的指令。
//!
//! 生成的文件带一行 `# yaml-language-server: $schema=./directive.schema.json`，并在同一
//! 目录放一份 schema 副本——编辑器于是立刻有补全、悬停文档与红波浪线，不用用户自己去配。
//!
//! 向导那条路（“只放一个动作”）还会顺带把 `permissions:` 写对：动作要求哪些权限，
//! `Action::permissions()` 旁边就有现成的答案，没必要让作者去猜。

use crate::actions;
use crate::ask;
use crate::build_registry;
use crate::output::outln;
use crate::scheduler::Paths;
use crate::schema;
use crate::usage;
use anyhow::{Context, Result, bail};
use corex_core::{ActionMeta, ParamSchema, PermissionSet, SchemaType};
use std::path::{Path, PathBuf};

/// 模板正文里的指令名占位符。
///
/// 用 `__NAME__` 而不是 `{name}`：正文里到处是 `{{ … }}` 形式的 Jinja 表达式，
/// 让占位符与它们长得完全不像，就不必去数大括号。
const NAME_TOKEN: &str = "__NAME__";

/// 一个内置模板。
struct Blueprint {
    /// `-t` 接受的名字，也是选单里的首列
    name: &'static str,
    /// 一句话说明，出现在选单里
    summary: &'static str,
    /// YAML 正文，含 `__NAME__` 占位
    body: &'static str,
}

/// 向导里的最后一个选项：不套模板，只放一个动作。
const ONE_ACTION: &str = "只放一个动作（从注册表挑，参数逐个问）";

/// 内置模板表。加模板就往这里加一行，别处不用改。
const BLUEPRINTS: &[Blueprint] = &[
    Blueprint {
        name: "hello",
        summary: "渲染问候语并写入文件（入门）",
        body: r#"# __NAME__ — 渲染问候语并写入文件
#
# 运行:
#   corex run __NAME__
#   corex run __NAME__ -i who=Alice
name: __NAME__
description: 渲染问候语并写入文件
version: "1.0"

inputs:
  - name: who
    description: 问候对象
    required: false
    default: "world"
  - name: out
    description: 输出文件路径
    required: false
    default: "{{env.TEMP}}/corex-__NAME__.txt"

permissions:
  filesystem: true

steps:
  - id: greet
    action: template.render
    params:
      template: "Hello, {{ who }}!"
      context:
        who: "{{input.who}}"
    save_to: message

  - id: write
    action: file.write
    params:
      path: "{{input.out}}"
      content: "{{message}}"
    save_to: written
"#,
    },
    Blueprint {
        name: "http",
        summary: "发一个 HTTP 请求并取出响应",
        body: r#"# __NAME__ — 发一个 HTTP 请求
#
# 运行:
#   corex run __NAME__
#   corex run __NAME__ -i url=https://api.github.com
name: __NAME__
description: 发一个 HTTP 请求并取出响应
version: "1.0"

inputs:
  - name: url
    description: 请求 URL
    required: false
    default: "https://api.github.com/repos/rust-lang/rust"

permissions:
  network: true

steps:
  - id: fetch
    action: http.send
    params:
      url: "{{input.url}}"
      method: GET
      timeout_ms: 15000
    save_to: response

  - id: show
    action: template.render
    params:
      template: "HTTP {{ response.status }} — {{ response.url }}"
      context:
        response: "{{response}}"
    save_to: summary
"#,
    },
    Blueprint {
        name: "file",
        summary: "读文件 → 处理 → 另存",
        body: r#"# __NAME__ — 读一个文件再另存
#
# 运行:
#   corex run __NAME__
#   corex run __NAME__ -i src=./README.md
name: __NAME__
description: 读文件 → 处理 → 另存
version: "1.0"

inputs:
  - name: src
    description: 源文件
    required: false
    default: "./README.md"
  - name: dst
    description: 输出文件
    required: false
    default: "{{env.TEMP}}/corex-__NAME__.txt"

permissions:
  filesystem: true

steps:
  - id: read
    action: file.read
    params:
      path: "{{input.src}}"
    save_to: text

  - id: write
    action: file.write
    params:
      path: "{{input.dst}}"
      content: "{{text}}"
    save_to: written
"#,
    },
    Blueprint {
        name: "shell",
        summary: "执行一条命令并保存输出",
        body: r#"# __NAME__ — 执行一条命令
#
# 运行:
#   corex run __NAME__
#   corex run __NAME__ -i command="git status --short"
name: __NAME__
description: 执行一条命令并保存输出
version: "1.0"

inputs:
  - name: command
    description: 要执行的命令
    required: false
    default: "echo hello from corex"

permissions:
  shell: true

steps:
  - id: run
    action: shell.run
    params:
      command: "{{input.command}}"
    save_to: result
"#,
    },
    Blueprint {
        name: "cron",
        summary: "定时触发（triggers.cron）",
        body: r#"# __NAME__ — 定时触发
#
# 运行:
#   corex cron run __NAME__        交给 cron supervisor
#   corex run __NAME__             手动跑一次
name: __NAME__
description: 定时触发的骨架
version: "1.0"

triggers:
  - type: cron
    expr: "0 9 * * 1-5"
    timezone: local

steps:
  - id: tick
    action: template.render
    params:
      template: "__NAME__ 被 cron 触发了"
    save_to: note
"#,
    },
    Blueprint {
        name: "watch",
        summary: "文件变化触发（triggers.watch）",
        body: r#"# __NAME__ — 文件变化触发
#
# 运行:
#   corex watch run __NAME__       交给 watch supervisor
#   corex run __NAME__             手动跑一次
name: __NAME__
description: 文件变化触发的骨架
version: "1.0"

triggers:
  - type: watch
    paths: ["./src"]
    debounce_ms: 500
    throttle_ms: 1200

steps:
  - id: changed
    action: template.render
    params:
      template: "__NAME__ 被文件变化触发了"
    save_to: note
"#,
    },
];

/// 生成一条指令；`name` / `template` 省略时在终端里问。
pub(crate) fn cmd_create(
    name: Option<&str>,
    template: Option<&str>,
    force: bool,
    dir: Option<&Path>,
) -> Result<()> {
    let base = Paths::dir(dir)?;
    let name = match name {
        Some(name) => name.trim().to_string(),
        None if ask::is_interactive() => ask::text("指令名？", None)?.trim().to_string(),
        None => return Err(usage("需要指令名，例如 corex create hello")),
    };
    if name.is_empty() {
        return Err(usage("指令名不能为空"));
    }

    let path = base.join(format!("{name}.yaml"));
    if path.exists() && !force {
        if !ask::is_interactive() {
            bail!("已存在: {}（要覆盖加 --force）", path.display());
        }
        if !ask::confirm(&format!("{} 已存在，覆盖它？", path.display()), false)? {
            outln!("已取消，未改动 {}", path.display());
            return Ok(());
        }
    }

    let body = compose(&name, template)?;
    // 先落 schema 副本：它写不进去就什么都别建，免得留下一个引用了空路径的文件。
    schema::seed(&base)?;
    std::fs::write(&path, body).with_context(|| format!("无法写入 {}", path.display()))?;

    outln!("已创建 {}", path.display());
    outln!("  corex edit {name}        用编辑器打开");
    outln!("  corex run {name}         立刻跑一遍");
    outln!("  corex validate {} --strict", path.display());
    Ok(())
}

/// 决定正文：`-t` 指名了就照着来，否则在终端里问。
fn compose(name: &str, template: Option<&str>) -> Result<String> {
    if let Some(spec) = template {
        return blueprint(spec, name);
    }
    if !ask::is_interactive() {
        // 非交互时用第一个模板：脚本与 CI 不该被问题卡住。
        return blueprint(BLUEPRINTS[0].name, name);
    }

    let mut labels: Vec<String> = BLUEPRINTS
        .iter()
        .map(|b| format!("{:<8} {}", b.name, b.summary))
        .collect();
    labels.push(ONE_ACTION.to_string());
    let chosen = ask::select("从哪个起点开始？", &labels)?;
    match BLUEPRINTS.get(chosen) {
        Some(found) => blueprint(found.name, name),
        None => wizard(name),
    }
}

/// 按内置模板名或一个 YAML 路径取正文。
fn blueprint(spec: &str, name: &str) -> Result<String> {
    match BLUEPRINTS.iter().find(|b| b.name == spec) {
        Some(found) => Ok(header() + &found.body.replace(NAME_TOKEN, name)),
        None => copied(spec, name),
    }
}

/// 从外部 YAML 取正文：原样拷过来，只在里面有 `__NAME__` 时替换。
///
/// 不做 YAML 重写——用户给的文件是他的，`name:` 字段对不上文件名这种事，
/// 看得出来，也轮不到我们来猜。
fn copied(spec: &str, name: &str) -> Result<String> {
    let mut path = PathBuf::from(spec);
    if path.is_dir() {
        path = path.join(format!("{name}.yaml"));
    }
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "无法读取模板 {}（内置名: {}）",
            path.display(),
            builtin_names()
        )
    })?;
    Ok(text.replace(NAME_TOKEN, name))
}

/// 向导：挑一个动作，逐个问必填参数，并把权限写对。
fn wizard(name: &str) -> Result<String> {
    let registry = build_registry();
    let actions = registry.actions();
    let labels: Vec<String> = actions
        .iter()
        .map(|meta| format!("{:<22} [{}] {}", meta.id, meta.bucket, meta.description))
        .collect();
    let chosen = ask::select("用什么动作起头？", &labels)?;
    let meta = &actions[chosen];

    let mut out = header();
    out.push_str(&format!("name: {name}\n"));
    out.push_str("description: \"\"\n");
    out.push_str(&permissions(&registry, &meta.id));
    out.push_str("steps:\n");
    out.push_str(&step(meta)?);
    Ok(out)
}

/// 按动作声明的权限要求生成 `permissions:` 块。
fn permissions(registry: &corex_registry::ActionRegistry, id: &str) -> String {
    let declared = registry
        .get(id)
        .map(|action| action.permissions())
        .unwrap_or(PermissionSet::NONE);
    let mut out = String::new();
    if declared.is_empty() {
        // 什么都不需要就写空 map：留空会让 `permissions` 缺失，进而变成 allow-all。
        return "permissions: {}\n".to_string();
    }
    out.push_str("permissions:\n");
    for kind in declared.iter() {
        out.push_str(&format!("  {}: true\n", kind.name()));
    }
    out
}

/// 一个动作步骤：必填参数问一遍，可选参数留成注释。
fn step(meta: &ActionMeta) -> Result<String> {
    let mut out = format!(
        "  - id: {}\n    action: {}\n    params:\n",
        actions::short_id(&meta.id),
        meta.id
    );
    for param in &meta.params {
        if param.required {
            let answer = ask_param(param)?;
            out.push_str(&format!(
                "      {}: {}\n",
                param.name,
                scalar(&answer, param.ty)
            ));
        } else {
            out.push_str(&format!(
                "      # {}: {}\n",
                param.name,
                actions::placeholder(param.ty)
            ));
        }
    }
    Ok(out)
}

/// 参数提示语：带上描述与类型，用户不用回去翻文档。
fn prompt(param: &ParamSchema) -> String {
    let detail = param.description.clone().unwrap_or_default();
    if detail.is_empty() {
        format!("{}（{}）", param.name, param.ty.as_str())
    } else {
        format!("{}（{}，{}）", param.name, param.ty.as_str(), detail)
    }
}

/// 按参数类型挑追问方式：密钥类不回显。
fn ask_param(param: &ParamSchema) -> Result<String> {
    match param.ty {
        SchemaType::Secret => ask::password(&prompt(param)),
        _ => ask::text(&prompt(param), None),
    }
}

/// 把用户敲的值写成 YAML 标量：数字与布尔裸写，其余加引号。
fn scalar(answer: &str, ty: SchemaType) -> String {
    let numeric =
        matches!(ty, SchemaType::Int | SchemaType::Float) && answer.parse::<f64>().is_ok();
    let boolean = matches!(ty, SchemaType::Bool) && matches!(answer, "true" | "false");
    if numeric || boolean {
        answer.to_string()
    } else {
        format!("\"{}\"", answer.replace('"', "\\\""))
    }
}

/// 生成的 YAML 顶部那行 schema 提示，让编辑器直接拿到补全与校验。
fn header() -> String {
    format!("{}\n", schema::HINT)
}

fn builtin_names() -> String {
    BLUEPRINTS
        .iter()
        .map(|b| b.name)
        .collect::<Vec<_>>()
        .join(" / ")
}
