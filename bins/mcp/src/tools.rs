//! 把 `ActionRegistry` 摊成 MCP 的 tool 清单。
//!
//! 事实来源只有一处：`corex_registry::catalog`（与 `corex actions --json`、
//! daemon 的 `list_actions` 共用）。这里只负责「corex 的动作描述 → MCP 的
//! [`Tool`]」，不另建一份参数表——目录一旦分叉，agent 看到的参数表就会与
//! CLI 打印的不一致。

use corex_registry::ActionRegistry;
use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

/// 跑整条指令的 meta-tool。前缀 `corex_` 避开与任何动作 id 撞名。
pub const DIRECTIVE_TOOL: &str = "corex_run_directive";

/// 动作 id（`file.copy`）→ MCP tool 名（`file_copy`）。
///
/// MCP tool 名惯例是 snake_case；动作 id 是点分命名。点换成下划线即可，不区分
/// 大小写、不改其它字符。
pub fn tool_name(action_id: &str) -> String {
    action_id.replace('.', "_")
}

/// 反向查表：tool 名 → 动作 id。逐个比对而不是建反向 map——80 个动作，一次
/// `tools/call` 的线性扫描不值一提，且不需要维护一份会失步的映射。
pub fn find_action_id(registry: &ActionRegistry, name: &str) -> Option<String> {
    registry
        .actions()
        .iter()
        .find(|meta| tool_name(&meta.id) == name)
        .map(|meta| meta.id.clone())
}

/// 全部内置动作各一个 tool，末尾补上 [`DIRECTIVE_TOOL`]。
pub fn build(registry: &ActionRegistry) -> Vec<Tool> {
    let mut tools: Vec<Tool> = registry
        .actions()
        .iter()
        .map(|meta| {
            let mut tool = Tool::new(
                tool_name(&meta.id),
                meta.description.clone(),
                json_object(corex_registry::catalog::input_schema(meta)),
            );
            tool.title = Some(meta.name.clone());
            tool.annotations = destructive_hint(&meta.id);
            tool
        })
        .collect();
    tools.push(directive_tool());
    tools
}

/// 破坏性动作打 `destructiveHint`：客户端据此在调用前弹确认（企业场景的「人在环」）。
///
/// 这是**提示**，不是门禁——真正的拦截靠 `strict_permissions` / `disabled_actions`
/// / `filesystem_roots`，执行路径与 CLI 完全一致。
fn destructive_hint(action_id: &str) -> Option<ToolAnnotations> {
    let destructive = action_id.ends_with(".remove")
        || action_id.ends_with(".delete")
        || action_id.ends_with(".write")
        || action_id.ends_with(".set")
        || action_id.ends_with(".clear")
        || action_id.starts_with("exec.")
        || action_id.starts_with("shell.")
        || action_id.starts_with("process.")
        || action_id.starts_with("ui.")
        || action_id.starts_with("capture.");
    destructive.then(|| {
        let mut annotations = ToolAnnotations::default();
        annotations.destructive_hint = Some(true);
        annotations
    })
}

fn directive_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "指令名（不含 .yaml 后缀），在 <数据目录>/directives 下按名查找"
            },
            "path": {
                "type": "string",
                "description": "可选：指令 YAML 路径（限 directives 根目录之内），给 path 时忽略 name"
            },
            "input": {
                "type": "object",
                "description": "可选：传给指令的输入，即 `corex run -i KEY=VALUE` 对应的 JSON 对象"
            }
        },
        "required": ["name"]
    });
    let mut tool = Tool::new(
        DIRECTIVE_TOOL,
        "按名称或路径执行一条 corex 指令（Directive YAML），返回结果 JSON。权限与配置同 corex CLI。",
        json_object(input),
    );
    tool.title = Some("运行指令".to_string());
    tool.annotations = Some({
        let mut annotations = ToolAnnotations::default();
        annotations.destructive_hint = Some(true);
        annotations
    });
    tool
}

/// `serde_json::Value` → rmcp 的 `JsonObject`；非对象退化成空对象（不应发生）。
fn json_object(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry() -> ActionRegistry {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        registry
    }

    #[test]
    fn tool_name_replaces_dots() {
        assert_eq!(tool_name("file.copy"), "file_copy");
        assert_eq!(tool_name("codec.base64.encode"), "codec_base64_encode");
    }

    #[test]
    fn find_action_id_roundtrips_for_every_action() {
        let registry = registry();
        for meta in registry.actions().iter() {
            let name = tool_name(&meta.id);
            assert_eq!(
                find_action_id(&registry, &name).as_deref(),
                Some(meta.id.as_str())
            );
        }
        assert!(find_action_id(&registry, "no_such_tool").is_none());
    }

    #[test]
    fn build_has_directive_tool_and_no_name_collisions() {
        let registry = registry();
        let tools = build(&registry);
        // 内置动作数 + 1（corex_run_directive）
        assert_eq!(tools.len(), registry.len() + 1);
        assert!(tools.iter().any(|t| t.name == DIRECTIVE_TOOL));
        // 工具名无重复（点→下划线映射必须保持单射，否则 tools/call 无法反查）
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(names.len(), total);
    }

    #[test]
    fn destructive_hint_flags_write_remove_and_shell_but_not_read() {
        assert!(destructive_hint("file.remove").is_some());
        assert!(destructive_hint("file.write").is_some());
        assert!(destructive_hint("shell.run").is_some());
        assert!(destructive_hint("capture.screenshot").is_some());
        assert!(destructive_hint("file.read").is_none());
    }
}
