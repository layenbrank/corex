//! 执行面：单动作直调 + 指令运行，与 `corex-daemon` 同一条路径。
//!
//! 刻意**内嵌**引擎（像 `corex run`，不像 `corex run --remote`）：权限、审计、历史
//! 都走 `corex-engine` 的既有实现，不另写一套。代价是拿不到 WASM 插件——与
//! `corex run` 的限制一致，将来需要时再加「桥接 daemon」的开关。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use corex_core::{ExecutionContext, RuntimeConfig, Value, check_runtime_allowed};
use corex_engine::{AuditEntry, Directive, ExecutionAudit, ExecutionHistory, Pipeline};
use corex_registry::ActionRegistry;

/// 一次 MCP server 进程持有的状态：注册表、配置、指令目录、历史与审计。
pub struct State {
    pub registry: Arc<ActionRegistry>,
    pub config: RuntimeConfig,
    pub directives_dir: PathBuf,
    pub history: Option<ExecutionHistory>,
    pub audit: Option<ExecutionAudit>,
}

impl State {
    /// 与 daemon 相同的装配顺序：注册内置动作 → 应用 `disabled_actions` →
    /// 打开历史与审计。配置的读取（`corex_core::config::read`）在 `main.rs`。
    pub fn build(config: RuntimeConfig, data: &Path, directives: Option<PathBuf>) -> Result<Self> {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        registry.remove_disabled(&config.plugins);

        // 与 daemon 同一规则：默认目录为空时放入起步指令；`--directives` 是调用方自己指的
        // 目录，不碰。
        let directives_dir = match directives {
            Some(dir) => {
                std::fs::create_dir_all(&dir)?;
                dir
            }
            None => {
                let dir = data.join("directives");
                if let Err(e) = corex_engine::starter::seed(&dir) {
                    tracing::warn!(error = %e, dir = %dir.display(), "起步指令写入失败");
                }
                dir
            }
        };

        let history = open_history(data, &config)?;
        let audit = ExecutionAudit::under_data_dir(data).ok();

        Ok(Self {
            registry: Arc::new(registry),
            config,
            directives_dir,
            history,
            audit,
        })
    }

    /// 按 id 调用单个动作。镜像 `bins/daemon/src/main.rs::invoke_action`，去掉观察者。
    pub async fn invoke_action(&self, action_id: &str, params: Value) -> Result<Value> {
        // 严格模式 + 配置层停用；保留 `ActionError` 类型供上层取 `kind()`。
        check_runtime_allowed(&self.config, &*self.registry, action_id)?;
        let action = self
            .registry
            .get(action_id)
            .with_context(|| format!("动作未注册: {action_id}"))?;

        let t0 = Instant::now();
        let mut ctx = ExecutionContext::new(self.config.clone());
        // 让动作的 `ctx.chunk()` 知道自己属于哪一步（单动作直调绕过了 Pipeline）。
        ctx.enter_step("invoke", action_id);
        let outcome = async {
            action.validate(&params).await?;
            action.execute(params, &mut ctx).await
        }
        .await;
        ctx.leave_step();
        let duration_ms = t0.elapsed().as_millis() as u64;

        if let Some(audit) = &self.audit {
            let entry = AuditEntry::from_action(
                "invoke",
                "invoke",
                action_id,
                duration_ms,
                outcome.as_ref().map(|_| ()),
            );
            audit.record_best_effort(&entry);
        }
        Ok(outcome?)
    }

    /// 按名或路径执行一条指令。镜像 `bins/daemon/src/main.rs::run_directive`，去掉观察者。
    pub async fn run_directive(
        &self,
        name: &str,
        path: Option<&str>,
        input: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Value> {
        let file = if let Some(p) = path {
            corex_core::path::confine_under(&self.directives_dir, Path::new(p))
                .map_err(|e| anyhow::anyhow!(e.0))
                .with_context(|| format!("指令路径越界: {p}"))?
        } else {
            resolve_directive(&self.directives_dir, name)?
        };
        let directive = Directive::from_yaml_file(&file)?;
        let input = input
            .into_iter()
            .map(|(k, v)| (k, Value::from_json(v)))
            .collect();
        let ctx = ExecutionContext::new(self.config.clone()).with_input(input);

        let mut pipeline = Pipeline::new(self.registry.clone());
        if let Some(history) = &self.history {
            pipeline = pipeline.with_history(history.clone());
        }
        if let Some(audit) = &self.audit {
            pipeline = pipeline.with_audit(audit.clone());
        }
        Ok(pipeline.execute(&directive, ctx).await?)
    }
}

fn open_history(data: &Path, config: &RuntimeConfig) -> Result<Option<ExecutionHistory>> {
    if !config.history.enabled {
        return Ok(None);
    }
    let path = if config.history.file.is_absolute() {
        config.history.file.clone()
    } else {
        data.join(&config.history.file)
    };
    Ok(Some(
        ExecutionHistory::open(path).context("无法打开执行历史文件")?,
    ))
}

/// 按名称解析指令：只在 `dir` 下找 `{name}.yaml` / `{name}.yml`。
fn resolve_directive(dir: &Path, name: &str) -> Result<PathBuf> {
    if name.is_empty()
        || name.contains("..")
        || name.contains('/')
        || name.contains('\\')
        || Path::new(name).is_absolute()
    {
        bail!("非法指令名: {name}");
    }
    let yaml = dir.join(format!("{name}.yaml"));
    let yml = dir.join(format!("{name}.yml"));
    if yaml.is_file() {
        return Ok(yaml);
    }
    if yml.is_file() {
        return Ok(yml);
    }
    bail!("指令未找到: {name}");
}
