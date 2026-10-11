//! MCP 工具面：仅暴露渐进披露用的 meta-tool（个位数）。
//!
//! **禁止**把 registry 摊平成 `file_copy` 一类动作工具——那是已删除的旧面。

use rmcp::model::{JsonObject, Tool, ToolAnnotations};
use serde_json::Value;

pub const ACTIONS: &str = "corex_actions";
pub const ACTION: &str = "corex_action";
pub const INVOKE: &str = "corex_invoke";
pub const DIRECTIVES: &str = "corex_directives";
pub const DIRECTIVE: &str = "corex_directive";
pub const VALIDATE: &str = "corex_validate";
pub const RUN: &str = "corex_run";

/// `tools/list` 唯一返回的工具集合（顺序稳定）。
pub fn meta_tools() -> Vec<Tool> {
    vec![
        actions_tool(),
        action_tool(),
        invoke_tool(),
        directives_tool(),
        directive_tool(),
        validate_tool(),
        run_tool(),
    ]
}

pub fn is_meta_tool(name: &str) -> bool {
    matches!(
        name,
        ACTIONS | ACTION | INVOKE | DIRECTIVES | DIRECTIVE | VALIDATE | RUN
    )
}

fn actions_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "bucket": {
                "type": "string",
                "description": "可选：按 bucket 过滤（system / network / data / ui / logic / plugin）"
            },
            "ids": {
                "type": "array",
                "items": { "type": "string" },
                "description": "可选：动作 id 白名单"
            },
            "limit": {
                "type": "integer",
                "description": "可选：最多返回条数（默认 100）"
            }
        }
    });
    let mut tool = Tool::new(
        ACTIONS,
        "取动作目录摘要（无完整 input_schema）。再对感兴趣的 id 调 corex_action。",
        json_object(input),
    );
    tool.title = Some("动作目录".into());
    tool.annotations = Some(ToolAnnotations::default().read_only(true));
    tool
}

fn action_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "动作 id，如 file.copy"
            }
        },
        "required": ["id"]
    });
    let mut tool = Tool::new(
        ACTION,
        "取单个动作的完整描述：input_schema、permissions、risk。",
        json_object(input),
    );
    tool.title = Some("动作详情".into());
    tool.annotations = Some(ToolAnnotations::default().read_only(true));
    tool
}

fn invoke_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "string",
                "description": "动作 id"
            },
            "params": {
                "type": "object",
                "description": "动作参数（形状见 corex_action 的 input_schema）",
                "additionalProperties": true
            }
        },
        "required": ["id"]
    });
    let mut tool = Tool::new(
        INVOKE,
        "执行单个动作。长任务同步阻塞直至完成。权限与 CLI 一致。",
        json_object(input),
    );
    tool.title = Some("调用动作".into());
    tool.annotations = Some(
        ToolAnnotations::default()
            .destructive(true)
            .open_world(true),
    );
    tool
}

fn directives_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "names": {
                "type": "array",
                "items": { "type": "string" },
                "description": "可选：指令名白名单"
            },
            "limit": {
                "type": "integer",
                "description": "可选：最多返回条数（默认 100）"
            }
        }
    });
    let mut tool = Tool::new(
        DIRECTIVES,
        "取指令库摘要（无 YAML）。再对感兴趣的名字调 corex_directive。",
        json_object(input),
    );
    tool.title = Some("指令目录".into());
    tool.annotations = Some(ToolAnnotations::default().read_only(true));
    tool
}

fn directive_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "name": {
                "type": "string",
                "description": "指令名（库内键）"
            }
        },
        "required": ["name"]
    });
    let mut tool = Tool::new(
        DIRECTIVE,
        "取单条指令：规范化 yaml + definition。",
        json_object(input),
    );
    tool.title = Some("指令详情".into());
    tool.annotations = Some(ToolAnnotations::default().read_only(true));
    tool
}

fn validate_tool() -> Tool {
    let input = serde_json::json!({
        "type": "object",
        "properties": {
            "definition": {
                "type": "object",
                "description": "指令 definition（JSON，与库内模型同形）",
                "additionalProperties": true
            },
            "strict": {
                "type": "boolean",
                "description": "为真时额外校验 permissions 声明（对齐 daemon validate）"
            }
        },
        "required": ["definition"]
    });
    let mut tool = Tool::new(
        VALIDATE,
        "校验指令 definition（动作已注册 / 允许 / 可选严格权限）。",
        json_object(input),
    );
    tool.title = Some("校验指令".into());
    tool.annotations = Some(ToolAnnotations::default().read_only(true));
    tool
}

fn run_tool() -> Tool {
    // oneOf：要么 name（库内），要么 path（ad-hoc YAML）；不再 required:["name"] 却允许 path。
    let input = serde_json::json!({
        "oneOf": [
            {
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "指令库中的名字"
                    },
                    "input": {
                        "type": "object",
                        "description": "可选：传给指令的输入",
                        "additionalProperties": true
                    }
                },
                "required": ["name"],
                "additionalProperties": false
            },
            {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "ad-hoc 指令 YAML 路径（不入库；企业 deny-default 下禁用）"
                    },
                    "input": {
                        "type": "object",
                        "description": "可选：传给指令的输入",
                        "additionalProperties": true
                    }
                },
                "required": ["path"],
                "additionalProperties": false
            }
        ]
    });
    let mut tool = Tool::new(
        RUN,
        "跑一条指令（name 或 path 二选一）。同步阻塞直至完成。",
        json_object(input),
    );
    tool.title = Some("运行指令".into());
    tool.annotations = Some(
        ToolAnnotations::default()
            .destructive(true)
            .open_world(true),
    );
    tool
}

fn json_object(value: Value) -> JsonObject {
    match value {
        Value::Object(map) => map,
        _ => JsonObject::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_tools_are_exactly_seven_with_stable_names() {
        let tools = meta_tools();
        assert_eq!(tools.len(), 7);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_ref()).collect();
        assert_eq!(
            names,
            vec![
                ACTIONS, ACTION, INVOKE, DIRECTIVES, DIRECTIVE, VALIDATE, RUN
            ]
        );
        assert!(!names.iter().any(|n| n.contains("run_directive")));
        assert!(!names.iter().any(|n| *n == "file_copy"));
    }

    #[test]
    fn run_schema_is_oneof_not_required_name() {
        let tool = meta_tools()
            .into_iter()
            .find(|t| t.name == RUN)
            .expect("corex_run");
        let schema = tool.input_schema.as_ref();
        assert!(schema.get("oneOf").is_some());
        assert!(schema.get("required").is_none());
    }
}
