//! Resolve directive variables for trigger configs at supervisor startup.

use crate::definition::Directive;
use crate::resolver::Resolver;
use crate::trigger::WatchConfig;
use corex_core::{EngineError, ExecutionContext, RuntimeConfig};

/// Seed top-level directive variables into an execution context.
pub fn seed_directive_variables(
    directive: &Directive,
    runtime: RuntimeConfig,
) -> Result<ExecutionContext, EngineError> {
    let mut ctx = ExecutionContext::new(runtime);
    for (k, v) in &directive.variables {
        let resolved = Resolver::resolve_value(v, &ctx)?;
        ctx.variables.entry(k.clone()).or_insert(resolved);
    }
    Ok(ctx)
}

fn resolve_string(ctx: &ExecutionContext, raw: &str) -> Result<String, EngineError> {
    Ok(Resolver::resolve_string(raw, ctx)?.to_string())
}

/// Resolve placeholders in a watch trigger config.
pub fn resolve_watch_config(
    directive: &Directive,
    runtime: RuntimeConfig,
    mut config: WatchConfig,
) -> Result<WatchConfig, EngineError> {
    let ctx = seed_directive_variables(directive, runtime)?;
    config.paths = config
        .paths
        .iter()
        .map(|p| resolve_string(&ctx, p))
        .collect::<Result<_, _>>()?;
    config.includes = config
        .includes
        .iter()
        .map(|p| resolve_string(&ctx, p))
        .collect::<Result<_, _>>()?;
    config.excludes = config
        .excludes
        .iter()
        .map(|p| resolve_string(&ctx, p))
        .collect::<Result<_, _>>()?;
    Ok(config)
}

/// Resolve placeholders in a cron expression.
pub fn resolve_cron_expr(
    directive: &Directive,
    runtime: RuntimeConfig,
    expr: &str,
) -> Result<String, EngineError> {
    let ctx = seed_directive_variables(directive, runtime)?;
    resolve_string(&ctx, expr)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::Directive;
    use corex_core::Value;

    #[test]
    fn resolve_watch_paths_from_variables() {
        let yaml = r#"
name: t
variables:
  root: "{{env.TEMP}}/watch-root"
steps:
  - id: a
    action: template.render
    params:
      template: ok
triggers:
  - type: watch
    paths: ["{{variables.root}}"]
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        let w = crate::trigger::find_watch_trigger(&d.triggers).unwrap().unwrap();
        let resolved = resolve_watch_config(&d, RuntimeConfig::default(), w).unwrap();
        assert!(!resolved.paths[0].contains("{{"));
        assert!(resolved.paths[0].contains("watch-root") || !resolved.paths[0].is_empty());
    }

    #[test]
    fn seed_variables_resolves_env() {
        let yaml = r#"
name: t
variables:
  base: "{{env.TEMP}}/foo"
steps:
  - id: a
    action: template.render
    params:
      template: ok
"#;
        let d = Directive::from_yaml_str(yaml).unwrap();
        let ctx = seed_directive_variables(&d, RuntimeConfig::default()).unwrap();
        let base = ctx.variables.get("base").unwrap();
        assert!(matches!(base, Value::Str(s) if !s.contains("{{")));
    }
}
