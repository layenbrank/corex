//! `template.render` — MiniJinja template rendering.

use crate::ActionRegistry;
use async_trait::async_trait;
use corex_core::{
    Action, ActionCategory, ActionError, ActionMeta, ExecutionContext, ParamSchema, SchemaType,
    Value,
};
use std::sync::Arc;

pub struct TemplateRender;

#[async_trait]
impl Action for TemplateRender {
    fn meta(&self) -> ActionMeta {
        ActionMeta::new(
            "template.render",
            "Template Render",
            "使用 MiniJinja 渲染模板字符串",
            ActionCategory::Data,
        )
        .with_params(vec![
            ParamSchema::new("template", SchemaType::Str, true),
            ParamSchema::new("context", SchemaType::Map, false)
                .with_default(Value::Map(Default::default())),
        ])
    }

    async fn execute(
        &self,
        params: Value,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, ActionError> {
        let map = params
            .as_map()
            .ok_or_else(|| ActionError::InvalidParams("需要 map 参数".into()))?;
        let template = map
            .get("template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ActionError::MissingParam("template".into()))?;

        let mut env = minijinja::Environment::new();
        env.add_template("tpl", template)
            .map_err(|e| ActionError::execution(format!("模板语法错误: {e}")))?;
        let tmpl = env
            .get_template("tpl")
            .map_err(|e| ActionError::execution(format!("加载模板失败: {e}")))?;

        // Build context: explicit context map + variables as fallback.
        let mut ctx_json = serde_json::Map::new();
        for (k, v) in &ctx.variables {
            ctx_json.insert(k.clone(), v.to_json());
        }
        for (k, v) in &ctx.input {
            ctx_json.insert(k.clone(), v.to_json());
        }
        if let Some(Value::Map(extra)) = map.get("context") {
            for (k, v) in extra {
                ctx_json.insert(k.clone(), v.to_json());
            }
        }

        let rendered = tmpl
            .render(serde_json::Value::Object(ctx_json))
            .map_err(|e| ActionError::execution(format!("渲染失败: {e}")))?;
        Ok(Value::Str(rendered))
    }
}

pub fn register(registry: &mut ActionRegistry) {
    registry.register(Arc::new(TemplateRender));
}
