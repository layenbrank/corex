//! 指令文档的 JSON Schema。
//!
//! 由定义格式的那些类型推导而来，因此发布出去的 `schemas/directive.schema.json`
//! 不可能描述出解析器会拒绍的形状。`Trigger` 是例外，它自己声明 schema，
//! 因为它的对外格式由手写的 `Serialize` / `Deserialize` 定义。
//!
//! 闸门是 `crates/engine/tests/directive_schema.rs`：入库文件漂移时它会失败，
//! `COREX_BLESS_SCHEMA=1` 可以重写它。

use crate::definition::Directive;

/// 编辑器可以固定引用的 id；改它就会弄坏既有集成。
const SCHEMA_ID: &str = "https://corex.local/schemas/directive.schema.json";

/// 指令文档 schema：美化排版、确定性输出、末尾换行。
pub fn directive_schema_json() -> String {
    let mut schema = schemars::schema_for!(Directive);
    let object = schema.ensure_object();
    object.insert("$id".into(), serde_json::json!(SCHEMA_ID));
    object.insert("title".into(), serde_json::json!("Corex Directive"));
    object.insert(
        "description".into(),
        serde_json::json!("YAML directive document executed by corex-engine"),
    );
    let mut json = serde_json::to_string_pretty(&schema.to_value()).expect("schema 必须可序列化");
    json.push('\n');
    json
}
