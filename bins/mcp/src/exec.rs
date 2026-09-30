//! 执行面：单动作直调 + 指令运行，与 `corex-daemon` 同一条路径。
//!
//! 刻意**内嵌**引擎（像 `corex run`，不像 `corex run --remote`）：权限、审计、历史
//! 都走 `corex-engine` 的既有实现，不另写一套。代价是拿不到 WASM 插件——与
//! `corex run` 的限制一致，将来需要时再加「桥接 daemon」的开关。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use corex_core::{EngineError, ExecutionContext, RuntimeConfig, Value, check_runtime_allowed};
use corex_engine::{AuditEntry, Directive, DirectiveSource, ExecutionAudit, HistorySink, Pipeline};
use corex_registry::ActionRegistry;
use corex_store::{
    BootstrapOptions, DirectiveStore, ImportOptions, StoreDirectiveSource, history_sink,
};

/// 一次 MCP server 进程持有的状态：注册表、配置、指令库、执行日志与审计。
pub struct State {
    pub registry: Arc<ActionRegistry>,
    pub config: RuntimeConfig,
    /// 指令的唯一真相源；`--dir` 里的 YAML 只是一次性导入的来源，不是第二个指令根。
    store: Arc<DirectiveStore>,
    source: Arc<dyn DirectiveSource>,
    history: Option<Arc<dyn HistorySink>>,
    pub audit: Option<ExecutionAudit>,
}

impl State {
    /// 与 daemon 相同的装配顺序：注册内置动作 → 应用 `disabled_actions` → 打开指令库与审计。
    /// 配置的读取（`corex_core::config::read`）在 `main.rs`。
    ///
    /// `import` 给的是**启动时一次性导入**的目录或文件（等价于 `corex directive import`）：
    /// v13 起指令的根只有一个，就是库；指向别处的 YAML 只有收进来才有意义。
    pub fn build(config: RuntimeConfig, data: &Path, import: Option<&Path>) -> Result<Self> {
        let mut registry = ActionRegistry::new();
        registry.register_builtins();
        registry.remove_disabled(&config.plugins);
        let registry = Arc::new(registry);

        let validate = corex_engine::admission(Arc::clone(&registry));
        let (store, _report) = DirectiveStore::open_in_data_dir(
            data,
            BootstrapOptions::from_config(data, &config),
            &validate,
        )
        .context("无法打开指令库")?;
        let store = Arc::new(store);

        if let Some(target) = import {
            let report = store
                .import_path(target, &ImportOptions::default(), &validate)
                .with_context(|| format!("导入指令失败: {}", target.display()))?;
            tracing::info!(
                path = %target.display(),
                created = report.created(),
                updated = report.updated(),
                skipped = report.skipped(),
                failed = report.failed(),
                "已导入指令"
            );
        }

        let audit = ExecutionAudit::under_data_dir(data).ok();
        Ok(Self {
            registry,
            config: config.clone(),
            source: Arc::new(StoreDirectiveSource::new(Arc::clone(&store))),
            history: history_sink(store.clone(), &config),
            store,
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

    /// 按名（或显式给的 ad-hoc 文件）执行一条指令。镜像 `bins/daemon/src/main.rs::run_directive`。
    pub async fn run_directive(
        &self,
        name: &str,
        path: Option<&str>,
        input: serde_json::Map<String, serde_json::Value>,
    ) -> Result<Value> {
        // 给了 `path` 就是调用方明确指的 ad-hoc 文件（agent 生成的临时指令）：认它，不查库。
        let directive = match path {
            Some(p) => Directive::from_yaml_file(Path::new(p))?,
            None => self.source.load(name)?,
        };
        let input = input
            .into_iter()
            .map(|(k, v)| (k, Value::from_json(v)))
            .collect();
        let ctx = ExecutionContext::new(self.config.clone()).with_input(input);

        let mut pipeline = Pipeline::new(self.registry.clone());
        if let Some(history) = &self.history {
            pipeline = pipeline.with_history(Arc::clone(history));
        }
        if let Some(audit) = &self.audit {
            pipeline = pipeline.with_audit(audit.clone());
        }
        Ok(pipeline.execute(&directive, ctx).await?)
    }

    /// 库里有多少条指令；`--dir` 导入之后报一句，让调用方知道收进来多少。
    pub fn directive_count(&self) -> usize {
        self.store.count().unwrap_or(0)
    }
}

/// 指令名必须是裸名：库里已经没有路径概念，但名字会被拿去拼临时文件名、写进 IPC 请求。
///
/// 与 `corex_store::validate_name` 同一套规则；这里留一层包装只是为了把错误折成
/// [`EngineError`]，让 MCP 与 CLI 对同一个名字问题给出同一种答复。
pub fn check_name(name: &str) -> Result<(), EngineError> {
    corex_store::validate_name(name).map_err(EngineError::from)
}
