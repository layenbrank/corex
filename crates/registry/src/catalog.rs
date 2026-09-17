//! 动作目录：把注册表里的事实摊成机器可读的 JSON。
//!
//! 同一个文档有三个消费方——`corex actions --json`、daemon 的 `list_actions`
//! 与 `corex mcp` 的 tool 清单。三者共用这一份实现，不许各自拼一遍：目录一旦分叉，
//! agent 看到的参数表就会与 CLI 打印的不一致。
//!
//! 形状以 `ActionMeta` 自己的 `Serialize` 为准（字段定义只有那一处），
//! 这里只补两样它没有的：`permissions`（声明在动作身上，见 `Action::permissions`）
//! 与 `input_schema`（从 `params` 派生，MCP 的 `inputSchema` 直接用它）。

use crate::ActionRegistry;
use corex_core::{ActionMeta, Bucket, SchemaType};
use serde_json::{Map, Value, json};

/// 生成这份目录的 corex 版本；宿主可以据此判断参数表要不要重新拉。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 目录文档：`{ version, count, bucket, actions: [...] }`。
///
/// `bucket` 给了就只收这一组，并**原样回报**筛选条件——读方不必猜手里这份
/// 是全量还是子集。
pub fn document(registry: &ActionRegistry, bucket: Option<Bucket>) -> Value {
    let actions = actions(registry, bucket);
    json!({
        "version": VERSION,
        "count": actions.len(),
        "bucket": bucket.map(Bucket::as_str),
        "actions": actions,
    })
}

/// 全部动作，按 id 排序（`ActionRegistry::actions` 已经排好）。
pub fn actions(registry: &ActionRegistry, bucket: Option<Bucket>) -> Vec<Value> {
    registry
        .actions()
        .iter()
        .filter(|meta| bucket.is_none_or(|only| only == meta.bucket))
        .map(|meta| action(registry, meta))
        .collect()
}

/// 单个动作的完整描述。
pub fn action(registry: &ActionRegistry, meta: &ActionMeta) -> Value {
    let mut doc = fields(meta);
    doc.insert(
        "permissions".into(),
        json!(permission_names(registry, &meta.id)),
    );
    doc.insert("input_schema".into(), input_schema(meta));
    Value::Object(doc)
}

/// `ActionMeta` 自己序列化出来的那些字段。
///
/// 刻意不在这里重抄一遍字段名：那会变成第二个事实来源，`ActionMeta` 以后加字段，
/// 目录会静默落后。
fn fields(meta: &ActionMeta) -> Map<String, Value> {
    match serde_json::to_value(meta) {
        Ok(Value::Object(map)) => map,
        // 只有非有限的浮点默认值（NaN / ∞）能让它失败，而那是动作自己的声明错误，
        // 不该让整份目录跟着消失——退化成仍认得出是谁的最小描述即可。
        _ => Map::from_iter([
            ("id".to_string(), json!(meta.id)),
            ("name".to_string(), json!(meta.name)),
        ]),
    }
}

/// 动作声明的权限类别名，顺序同 `PermissionKind::GRANTABLE`。
///
/// 文本输出与 JSON 目录都从这里取，免得两处各遍历一次 `permissions()`。
pub fn permission_names(registry: &ActionRegistry, id: &str) -> Vec<&'static str> {
    registry
        .get(id)
        .map(|action| {
            action
                .permissions()
                .iter()
                .map(|kind| kind.name())
                .collect()
        })
        .unwrap_or_default()
}

/// 从 `params` 派生的 JSON Schema，供 MCP 的 `inputSchema` 直接使用。
///
/// 只表达**形状**（类型、必填、默认值、说明）。动作之间的语义约束散在各自的
/// `validate` 里，静态 schema 表达不了，所以这里也不假装能表达。
///
/// 刻意**不写** `additionalProperties: false`：引擎的默认校验只查必填项，多给的参数
/// 会被忽略。声明成拒绝会让校验严格的宿主退掉本来能跑通的调用——与
/// 「权限宁可少报也不要多报」是同一个道理。
pub fn input_schema(meta: &ActionMeta) -> Value {
    let mut properties = Map::new();
    for param in &meta.params {
        let mut spec = Map::new();
        if let Some(label) = json_type(param.ty) {
            spec.insert("type".into(), json!(label));
        }
        if let Some(format) = json_format(param.ty) {
            spec.insert("format".into(), json!(format));
        }
        if let Some(description) = &param.description {
            spec.insert("description".into(), json!(description));
        }
        if let Some(default) = &param.default {
            spec.insert("default".into(), default.to_json());
        }
        properties.insert(param.name.clone(), Value::Object(spec));
    }

    let mut schema = Map::new();
    schema.insert("type".into(), json!("object"));
    schema.insert("properties".into(), Value::Object(properties));
    let required: Vec<&str> = meta
        .params
        .iter()
        .filter(|param| param.required)
        .map(|param| param.name.as_str())
        .collect();
    if !required.is_empty() {
        schema.insert("required".into(), json!(required));
    }
    Value::Object(schema)
}

/// `SchemaType` → JSON Schema 的 `type`；`None` 表示不限类型（`any`）。
///
/// 写成穷尽的 `match` 而不是 `const` 表 + 查找：表在新增一种 `SchemaType` 时会
/// 静默落进兜底分支，而这里会直接编译不过。
fn json_type(ty: SchemaType) -> Option<&'static str> {
    match ty {
        SchemaType::Null => Some("null"),
        SchemaType::Bool => Some("boolean"),
        SchemaType::Int => Some("integer"),
        SchemaType::Float => Some("number"),
        SchemaType::Str | SchemaType::Secret | SchemaType::File | SchemaType::Bytes => {
            Some("string")
        }
        SchemaType::Array => Some("array"),
        SchemaType::Map => Some("object"),
        SchemaType::Any => None,
    }
}

/// 需要额外点明的类型：`file` 是路径而不是任意字符串，`secret` 不该写死在指令里。
///
/// `format` 在 JSON Schema 里只是注解，读方不认识就忽略，所以这里只放
/// 有把握的两种。
fn json_format(ty: SchemaType) -> Option<&'static str> {
    match ty {
        SchemaType::File => Some("path"),
        SchemaType::Secret => Some("password"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtins() -> ActionRegistry {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        registry
    }

    /// 目录里每个动作都得有 id / bucket / 参数表 / 权限 / inputSchema，
    /// 才谈得上「够 agent 用」。
    #[test]
    fn every_action_is_described() {
        let registry = builtins();
        let doc = document(&registry, None);
        assert_eq!(doc["count"], registry.len());
        assert_eq!(doc["version"], VERSION);
        assert!(doc["bucket"].is_null());
        let actions = doc["actions"].as_array().expect("actions 是数组");
        assert_eq!(actions.len(), registry.len());
        for item in actions {
            for key in [
                "id",
                "name",
                "description",
                "bucket",
                "params",
                "permissions",
                "input_schema",
            ] {
                assert!(!item[key].is_null(), "{} 缺 {key}", item["id"]);
            }
        }
    }

    /// `--bucket` 只收一组，并把筛选条件回报出来。
    #[test]
    fn bucket_filters_and_is_echoed() {
        let registry = builtins();
        let doc = document(&registry, Some(Bucket::Ui));
        let actions = doc["actions"].as_array().expect("actions 是数组");
        assert!(!actions.is_empty(), "ui 组不该是空的");
        assert!(actions.iter().all(|item| item["bucket"] == json!("ui")));
        assert_eq!(doc["count"], actions.len());
        assert_eq!(doc["bucket"], json!("ui"));
    }

    /// 参数表派生出的 `inputSchema`：类型、路径提示、说明与必填都在。
    #[test]
    fn input_schema_carries_types_and_required() {
        let registry = builtins();
        let meta = registry
            .actions()
            .into_iter()
            .find(|meta| meta.id == "file.copy")
            .expect("file.copy 已注册");
        let schema = input_schema(&meta);
        assert_eq!(schema["type"], json!("object"));
        assert_eq!(schema["properties"]["from"]["type"], json!("string"));
        assert_eq!(schema["properties"]["from"]["format"], json!("path"));
        assert_eq!(schema["required"], json!(["from", "to"]));
    }

    /// 不限类型的参数不该凭空写上一个 `type`。
    #[test]
    fn any_param_has_no_type_keyword() {
        let registry = builtins();
        let meta = registry
            .actions()
            .into_iter()
            .find(|meta| meta.id == "file.write")
            .expect("file.write 已注册");
        // `content` 是 `Any`：字符串与 `Bytes` 都收，写 `type: string` 是撒谎。
        let content = &input_schema(&meta)["properties"]["content"];
        assert!(content.get("type").is_none(), "content: {content}");
    }

    /// 权限取自动作自己的声明，不是另抄的一张表。
    #[test]
    fn permissions_come_from_the_action() {
        let registry = builtins();
        assert_eq!(permission_names(&registry, "file.copy"), vec!["filesystem"]);
        assert!(permission_names(&registry, "codec.json.parse").is_empty());
        assert!(permission_names(&registry, "nope.nope").is_empty());
    }
}
