//! 执行面：发现 + 校验 + invoke / run；与 daemon 同源权限与审计。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use corex_core::path::confine_in_roots;
use corex_core::{
    ActionError, Bucket, EngineError, ExecutionContext, RuntimeConfig, Value, check_runtime_allowed,
};
use corex_engine::{
    AuditEntry, Directive, DirectiveSource, ExecutionAudit, HistorySink, Pipeline,
    validate_allowed, validate_permissions, validate_registered,
};
use corex_registry::ActionRegistry;
use corex_store::{
    BootstrapOptions, DirectiveStore, ImportOptions, StoreDirectiveSource, history_sink,
};
use serde_json::json;

const ACTIONS_LIMIT: usize = 100;
const DIRECTIVES_LIMIT: usize = 100;

/// 一次 MCP server 进程持有的状态。
pub struct State {
    pub registry: Arc<ActionRegistry>,
    pub config: RuntimeConfig,
    store: Arc<DirectiveStore>,
    source: Arc<dyn DirectiveSource>,
    history: Option<Arc<dyn HistorySink>>,
    pub audit: Option<ExecutionAudit>,
}

impl State {
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

    pub fn session_tag() -> String {
        std::env::var("COREX_MCP_SESSION")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "mcp".into())
    }

    pub fn directive_count(&self) -> usize {
        self.store.count().unwrap_or(0)
    }

    /// 动作是否允许出现在目录 / 被 invoke（disabled + allowlist + deny-default + strict）。
    pub fn can_expose_action(&self, action_id: &str) -> bool {
        if !self.registry.contains(action_id) {
            return false;
        }
        if check_runtime_allowed(&self.config, &*self.registry, action_id).is_err() {
            return false;
        }
        let allow = &self.config.mcp.allow_actions;
        if self.config.mcp.is_deny_default && allow.is_empty() {
            return false;
        }
        if !allow.is_empty() && !allow.iter().any(|id| id == action_id) {
            return false;
        }
        true
    }

    pub fn can_expose_directive(&self, name: &str) -> bool {
        let allow = &self.config.mcp.allow_directives;
        if self.config.mcp.is_deny_default && allow.is_empty() {
            return false;
        }
        if !allow.is_empty() && !allow.iter().any(|n| n == name) {
            return false;
        }
        true
    }

    pub fn ensure_action_allowed(&self, action_id: &str) -> Result<(), ActionError> {
        if !self.registry.contains(action_id) {
            return Err(ActionError::NotFound(action_id.into()));
        }
        if self.can_expose_action(action_id) {
            return Ok(());
        }
        Err(ActionError::PermissionDenied(format!(
            "MCP 策略拒绝动作 {action_id}（allow_actions / is_deny_default / disabled / strict_permissions）"
        )))
    }

    pub fn ensure_directive_allowed(&self, name: &str) -> Result<(), ActionError> {
        if self.can_expose_directive(name) {
            return Ok(());
        }
        Err(ActionError::PermissionDenied(format!(
            "MCP 策略拒绝指令 {name}（allow_directives / is_deny_default）"
        )))
    }

    pub fn fetch_actions(
        &self,
        bucket: Option<&str>,
        ids: Option<&[String]>,
        limit: Option<usize>,
    ) -> Result<Value, EngineError> {
        let parsed = match bucket {
            None => None,
            Some(name) => Some(
                Bucket::parse(name)
                    .ok_or_else(|| EngineError::Usage(format!("未知 bucket: {name}")))?,
            ),
        };
        let allow_ids: Option<Vec<String>> = match ids {
            Some(wanted) => Some(
                wanted
                    .iter()
                    .filter(|id| self.can_expose_action(id))
                    .cloned()
                    .collect(),
            ),
            None if !self.config.mcp.allow_actions.is_empty()
                || self.config.mcp.is_deny_default =>
            {
                Some(
                    self.registry
                        .actions()
                        .iter()
                        .map(|meta| meta.id.clone())
                        .filter(|id| self.can_expose_action(id))
                        .collect(),
                )
            }
            None => {
                // 仍过滤 disabled / strict
                let filtered: Vec<String> = self
                    .registry
                    .actions()
                    .iter()
                    .map(|meta| meta.id.clone())
                    .filter(|id| self.can_expose_action(id))
                    .collect();
                // 若全部可暴露，传 None 让 catalog 走全表；否则传白名单
                if filtered.len() == self.registry.len() {
                    None
                } else {
                    Some(filtered)
                }
            }
        };
        let cap = limit.unwrap_or(ACTIONS_LIMIT);
        Ok(Value::from_json(corex_registry::catalog::fetch_actions(
            &self.registry,
            parsed,
            allow_ids.as_deref(),
            Some(cap),
        )))
    }

    pub fn find_action(&self, id: &str) -> Result<Value, ActionError> {
        self.ensure_action_allowed(id)?;
        corex_registry::catalog::find_action(&self.registry, id)
            .map(Value::from_json)
            .ok_or_else(|| ActionError::NotFound(id.into()))
    }

    pub fn fetch_directives(
        &self,
        names: Option<&[String]>,
        limit: Option<usize>,
    ) -> Result<Value, anyhow::Error> {
        let filtered: Option<Vec<String>> = match names {
            Some(wanted) => Some(
                wanted
                    .iter()
                    .filter(|name| self.can_expose_directive(name))
                    .cloned()
                    .collect(),
            ),
            None if !self.config.mcp.allow_directives.is_empty()
                || self.config.mcp.is_deny_default =>
            {
                let metas = self.store.metas()?;
                Some(
                    metas
                        .into_iter()
                        .map(|meta| meta.name)
                        .filter(|name| self.can_expose_directive(name))
                        .collect(),
                )
            }
            None => None,
        };
        // deny-default 且空 allow → 上面得到 Some([])；正常开放则 None
        if self.config.mcp.is_deny_default
            && self.config.mcp.allow_directives.is_empty()
            && filtered.as_ref().is_some_and(|v| v.is_empty())
        {
            return Ok(Value::from_json(json!({
                "count": 0,
                "directives": []
            })));
        }
        let mut doc = corex_store::catalog::fetch_directives(
            &self.store,
            filtered.as_deref(),
            Some(limit.unwrap_or(DIRECTIVES_LIMIT)),
        )?;
        // 再滤一遍（库里可能有 allow 外的）
        let retained = doc
            .get("directives")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|item| {
                        item.get("name")
                            .and_then(|v| v.as_str())
                            .is_some_and(|name| self.can_expose_directive(name))
                    })
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if let Some(obj) = doc.as_object_mut() {
            let count = retained.len();
            obj.insert("directives".into(), json!(retained));
            obj.insert("count".into(), json!(count));
        }
        Ok(Value::from_json(doc))
    }

    pub fn fetch_directive(&self, name: &str) -> Result<Value, anyhow::Error> {
        self.ensure_directive_allowed(name)?;
        Ok(Value::from_json(corex_store::catalog::fetch_directive(
            &self.store,
            name,
        )?))
    }

    pub fn validate_directive(
        &self,
        definition: serde_json::Value,
        strict: bool,
    ) -> Result<Value, anyhow::Error> {
        let directive: Directive = serde_json::from_value(definition)
            .map_err(|e| EngineError::parse(format!("指令定义不合法: {e}")))?;
        validate_registered(&*self.registry, &directive)?;
        validate_allowed(&*self.registry, &directive)?;
        if strict {
            validate_permissions(&*self.registry, &directive).map_err(EngineError::Usage)?;
        }
        // 策略：指令名若在 allow 外也提示（校验本身不执行）
        if !directive.name.is_empty() && !self.can_expose_directive(&directive.name) {
            bail!(ActionError::PermissionDenied(format!(
                "MCP 策略拒绝指令 {}",
                directive.name
            )));
        }
        Ok(Value::from_json(json!({
            "ok": true,
            "name": directive.name,
        })))
    }

    pub async fn invoke_action(&self, action_id: &str, params: Value, tool: &str) -> Result<Value> {
        self.ensure_action_allowed(action_id)?;
        check_runtime_allowed(&self.config, &*self.registry, action_id)?;
        let action = self
            .registry
            .get(action_id)
            .with_context(|| format!("动作未注册: {action_id}"))?;

        let t0 = Instant::now();
        let mut ctx = ExecutionContext::new(self.config.clone());
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
            )
            .with_mcp(tool, action_id, Self::session_tag());
            audit.record_best_effort(&entry);
        }
        Ok(outcome?)
    }

    pub async fn run_directive(
        &self,
        name: Option<&str>,
        path: Option<&str>,
        input: serde_json::Map<String, serde_json::Value>,
        tool: &str,
    ) -> Result<Value> {
        let path = match path {
            Some(p) => Some(self.resolve_run_path(p)?),
            None => None,
        };
        let directive = match (&path, name) {
            (Some(p), _) => Directive::from_yaml_file(p)?,
            (None, Some(n)) => {
                self.ensure_directive_allowed(n)?;
                check_name(n)?;
                self.source.load(n)?
            }
            (None, None) => {
                bail!(EngineError::Usage(
                    "corex_run 需要 name 或 path 之一".into()
                ));
            }
        };
        let target = path
            .as_ref()
            .map(|p| p.display().to_string())
            .or_else(|| name.map(str::to_string))
            .unwrap_or_default();

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
        let t0 = Instant::now();
        let outcome = pipeline.execute(&directive, ctx).await;
        let duration_ms = t0.elapsed().as_millis() as u64;
        if let Some(audit) = &self.audit {
            let mapped: Result<(), &EngineError> = match &outcome {
                Ok(_) => Ok(()),
                Err(e) => Err(e),
            };
            let entry = AuditEntry::from_engine(
                directive.name.as_str(),
                "run",
                "directive",
                duration_ms,
                mapped,
            )
            .with_mcp(tool, &target, Self::session_tag());
            audit.record_best_effort(&entry);
        }
        Ok(outcome?)
    }

    /// `path` 旁路：deny-default 禁止；否则若配置了 filesystem_roots 则 confine。
    fn resolve_run_path(&self, path: &str) -> Result<PathBuf> {
        if self.config.mcp.is_deny_default {
            bail!(ActionError::PermissionDenied(
                "is_deny_default 下禁止 corex_run 的 path 旁路，请用指令库 name".into()
            ));
        }
        let raw = PathBuf::from(path);
        if self.config.filesystem_roots.is_empty() {
            return Ok(raw);
        }
        confine_in_roots(&self.config.filesystem_roots, &raw)
            .map_err(|e| ActionError::PermissionDenied(e.to_string()).into())
    }
}

pub fn check_name(name: &str) -> Result<(), EngineError> {
    corex_store::validate_name(name).map_err(EngineError::from)
}
