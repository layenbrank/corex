//! Pipeline executor for directives.

use crate::audit::{self, AuditEntry, ExecutionAudit};
use crate::control_flow::evaluate_condition;
use crate::definition::{
    ActionStep, Directive, IfStep, OnError, ParallelStep, Permissions, RepeatStep, Step,
};
use crate::history::{ExecutionHistory, HistoryEntry};
use crate::inputs::apply_input_defaults;
use crate::resolver::Resolver;
use corex_core::{ActionError, ActionStore, EngineError, ExecutionContext, Value};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, error, info, warn};

/// Executes a [`Directive`] against an [`ActionStore`].
pub struct Pipeline {
    store: Arc<dyn ActionStore>,
    history: Option<ExecutionHistory>,
    audit: Option<ExecutionAudit>,
    /// Set during [`Self::execute`] for step audit / logs.
    run_name: Option<String>,
}

impl Pipeline {
    pub fn new(store: Arc<dyn ActionStore>) -> Self {
        Self {
            store,
            history: None,
            audit: None,
            run_name: None,
        }
    }

    /// Enable append-only JSONL recording for each [`Self::execute`] call.
    pub fn with_history(mut self, history: ExecutionHistory) -> Self {
        self.history = Some(history);
        self
    }

    /// Enable step-level redacted audit JSONL.
    pub fn with_audit(mut self, audit: ExecutionAudit) -> Self {
        self.audit = Some(audit);
        self
    }

    /// Execute an entire directive.
    pub async fn execute(
        &self,
        directive: &Directive,
        mut ctx: ExecutionContext,
    ) -> Result<Value, EngineError> {
        let started = SystemTime::now();
        let pipeline = Self {
            store: Arc::clone(&self.store),
            history: self.history.clone(),
            audit: self.audit.clone(),
            run_name: Some(directive.name.clone()),
        };

        for (k, v) in &directive.variables {
            let resolved = Resolver::resolve_value(v, &ctx)?;
            ctx.variables.entry(k.clone()).or_insert(resolved);
        }
        if let Err(e) = apply_input_defaults(directive, &mut ctx) {
            pipeline.record_history(directive, started, Err(&e));
            return Err(e);
        }

        info!(
            directive = %directive.name,
            steps = directive.steps.len(),
            "开始执行指令"
        );
        if ctx.config.strict_permissions && directive.permissions.is_unrestricted() {
            let err = EngineError::Action(corex_core::ActionError::PermissionDenied(
                "strict_permissions 已启用：指令必须声明 permissions".into(),
            ));
            pipeline.record_history(directive, started, Err(&err));
            return Err(err);
        }
        let result = pipeline
            .execute_steps(
                &directive.steps,
                &mut ctx,
                directive.on_error,
                &directive.permissions,
            )
            .await;

        match result {
            Ok(v) => {
                info!(directive = %directive.name, "指令执行完成");
                pipeline.record_history(directive, started, Ok(()));
                Ok(v)
            }
            Err(e) => {
                error!(directive = %directive.name, error = %e, "指令执行失败");
                pipeline.record_history(directive, started, Err(&e));
                Err(e)
            }
        }
    }

    fn record_history(
        &self,
        directive: &Directive,
        started: SystemTime,
        outcome: Result<(), &EngineError>,
    ) {
        let Some(history) = &self.history else {
            return;
        };
        let ended = SystemTime::now();
        let entry = HistoryEntry::new(&directive.name, started, ended, outcome);
        history.record_best_effort(&entry);
    }

    fn record_step_audit(
        &self,
        step: &ActionStep,
        duration_ms: u64,
        outcome: Result<(), &EngineError>,
    ) {
        let name = self.run_name.as_deref().unwrap_or("unknown");
        let entry = AuditEntry::from_engine(name, &step.id, &step.action, duration_ms, outcome);
        audit::log_step_end(&entry);
        if let Some(a) = &self.audit {
            a.record_best_effort(&entry);
        }
    }

    pub async fn execute_steps(
        &self,
        steps: &[Step],
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        let mut last = Value::Null;
        for step in steps {
            last = self
                .execute_step(step, ctx, default_on_error, permissions)
                .await?;
        }
        Ok(last)
    }

    pub async fn execute_step(
        &self,
        step: &Step,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        match step {
            Step::Action(s) => {
                self.run_action_step(s, ctx, default_on_error, permissions)
                    .await
            }
            Step::If(s) => {
                self.run_if_step(s, ctx, default_on_error, permissions)
                    .await
            }
            Step::Repeat(s) => {
                self.run_repeat_step(s, ctx, default_on_error, permissions)
                    .await
            }
            Step::Parallel(s) => {
                self.run_parallel_step(s, ctx, default_on_error, permissions)
                    .await
            }
        }
    }

    async fn run_action_step(
        &self,
        step: &ActionStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        if let Some(cond) = &step.when {
            if !evaluate_condition(cond, ctx)? {
                debug!(id = %step.id, "when 条件为假，跳过步骤");
                return Ok(Value::Null);
            }
        }

        let on_error = step.on_error.unwrap_or(default_on_error);
        let retries = step.retry.unwrap_or(0);
        let mut attempt = 0u32;

        loop {
            let outcome = self.invoke_action(step, ctx, permissions).await;
            match outcome {
                Ok(value) => {
                    ctx.set_step_output(&step.id, value.clone());
                    if let Some(save_to) = &step.save_to {
                        ctx.set_variable(save_to, value.clone());
                    }
                    return Ok(value);
                }
                Err(e) => {
                    if attempt < retries {
                        attempt += 1;
                        warn!(id = %step.id, attempt, error = %e, "步骤失败，重试中");
                        continue;
                    }
                    if e.is_permission_denied() {
                        return Err(e);
                    }
                    match on_error {
                        OnError::Abort => return Err(e),
                        OnError::Continue => {
                            warn!(id = %step.id, error = %e, "步骤失败，on_error=continue");
                            ctx.set_step_output(&step.id, Value::Null);
                            return Ok(Value::Null);
                        }
                        OnError::Skip => {
                            warn!(
                                id = %step.id,
                                error = %e,
                                "步骤失败，on_error=skip（不写入 step_outputs）"
                            );
                            return Ok(Value::Null);
                        }
                    }
                }
            }
        }
    }

    async fn invoke_action(
        &self,
        step: &ActionStep,
        ctx: &mut ExecutionContext,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        let t0 = Instant::now();
        let name = self.run_name.as_deref().unwrap_or("unknown");
        audit::log_step_start(name, &step.id, &step.action);

        if let Err(e) = permissions.allows_action(&step.action) {
            let err = EngineError::StepFailed {
                step: step.id.clone(),
                source: e,
            };
            self.record_step_audit(step, t0.elapsed().as_millis() as u64, Err(&err));
            return Err(err);
        }

        let action = match self.store.find_action(&step.action) {
            Some(a) => a,
            None => {
                let err = EngineError::ActionNotRegistered(step.action.clone());
                self.record_step_audit(step, t0.elapsed().as_millis() as u64, Err(&err));
                return Err(err);
            }
        };

        let params = match Resolver::resolve_value(&step.params, ctx) {
            Ok(p) => p,
            Err(e) => {
                self.record_step_audit(step, t0.elapsed().as_millis() as u64, Err(&e));
                return Err(e);
            }
        };
        if let Err(e) = action.validate(&params).await {
            let err = EngineError::StepFailed {
                step: step.id.clone(),
                source: e,
            };
            self.record_step_audit(step, t0.elapsed().as_millis() as u64, Err(&err));
            return Err(err);
        }

        debug!(id = %step.id, action = %step.action, "执行动作");
        let timeout_secs = ctx.config.step_timeout_secs;
        let fut = action.execute(params, ctx);
        let result = if timeout_secs > 0 {
            match tokio::time::timeout(Duration::from_secs(timeout_secs), fut).await {
                Ok(r) => r,
                Err(_) => Err(ActionError::Timeout(format!(
                    "步骤 {} 超过 {timeout_secs}s",
                    step.id
                ))),
            }
        } else {
            fut.await
        };

        let duration_ms = t0.elapsed().as_millis() as u64;
        match result {
            Ok(v) => {
                self.record_step_audit(step, duration_ms, Ok(()));
                Ok(v)
            }
            Err(e) => {
                let err = EngineError::StepFailed {
                    step: step.id.clone(),
                    source: e,
                };
                self.record_step_audit(step, duration_ms, Err(&err));
                Err(err)
            }
        }
    }

    async fn run_if_step(
        &self,
        step: &IfStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        let pass = evaluate_condition(&step.condition, ctx)?;
        debug!(id = %step.id, pass, "求值 if 条件");
        if pass {
            Box::pin(self.execute_steps(&step.then, ctx, default_on_error, permissions)).await
        } else {
            Box::pin(self.execute_steps(&step.else_steps, ctx, default_on_error, permissions)).await
        }
    }

    async fn run_repeat_step(
        &self,
        step: &RepeatStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        let mut last = Value::Null;
        if let Some(count) = step.repeat.count {
            let as_var = &step.repeat.as_var;
            for i in 0..count {
                ctx.set_variable(as_var, Value::Int(i as i64));
                last =
                    Box::pin(self.execute_steps(&step.steps, ctx, default_on_error, permissions))
                        .await?;
            }
        } else if let Some(each) = &step.repeat.each {
            let list_val = Resolver::resolve_string(each, ctx)?;
            let items = match list_val {
                Value::List(l) => l,
                other => {
                    return Err(EngineError::ControlFlow(format!(
                        "repeat.each 必须解析为列表，得到: {other}"
                    )));
                }
            };
            for (i, item) in items.into_iter().enumerate() {
                ctx.set_variable(&step.repeat.as_var, item);
                ctx.set_variable(&step.repeat.index_var, Value::Int(i as i64));
                last =
                    Box::pin(self.execute_steps(&step.steps, ctx, default_on_error, permissions))
                        .await?;
            }
        } else {
            return Err(EngineError::ControlFlow("repeat 需要 count 或 each".into()));
        }
        ctx.set_step_output(&step.id, last.clone());
        Ok(last)
    }

    async fn run_parallel_step(
        &self,
        step: &ParallelStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        use futures::stream::{self, StreamExt};

        let max = step
            .max_concurrency
            .unwrap_or(ctx.config.max_parallel)
            .max(1);
        let children = step.parallel.len();

        debug!(id = %step.id, max, children, "执行 parallel（buffer_unordered）");
        let store = Arc::clone(&self.store);
        let audit = self.audit.clone();
        let history = self.history.clone();
        let run_name = self.run_name.clone();
        let base_ctx = ctx.clone();
        let perms = permissions.clone();

        let collected: Vec<Result<(usize, ExecutionContext, Value), EngineError>> = stream::iter(
            step.parallel
                .iter()
                .cloned()
                .enumerate()
                .map(|(idx, child)| {
                    let store = Arc::clone(&store);
                    let audit = audit.clone();
                    let history = history.clone();
                    let run_name = run_name.clone();
                    let mut branch_ctx = base_ctx.clone();
                    let perms = perms.clone();
                    async move {
                        let mut pipeline = Pipeline::new(store);
                        if let Some(h) = history {
                            pipeline = pipeline.with_history(h);
                        }
                        if let Some(a) = audit {
                            pipeline = pipeline.with_audit(a);
                        }
                        pipeline.run_name = run_name;
                        let value = Box::pin(pipeline.execute_step(
                            &child,
                            &mut branch_ctx,
                            default_on_error,
                            &perms,
                        ))
                        .await?;
                        Ok::<_, EngineError>((idx, branch_ctx, value))
                    }
                }),
        )
        .buffer_unordered(max)
        .collect()
        .await;

        let mut successes: Vec<(usize, ExecutionContext, Value)> = Vec::new();
        let mut branch_err: Option<EngineError> = None;
        for item in collected {
            match item {
                Ok(v) => successes.push(v),
                Err(e) => branch_err = Some(prefer_branch_err(branch_err.take(), e)),
            }
        }
        successes.sort_by_key(|(idx, _, _)| *idx);

        if let Some(e) = branch_err {
            if must_abort_step(&e, default_on_error) {
                return Err(e);
            }
            warn!(id = %step.id, error = %e, "parallel 部分失败，按 on_error 继续");
        }

        let mut outputs: Vec<Option<Value>> = vec![None; children];
        for (idx, branch_ctx, value) in successes {
            ctx.merge_from_branch(&branch_ctx);
            if idx < outputs.len() {
                outputs[idx] = Some(value);
            }
        }

        let list: Vec<Value> = outputs
            .into_iter()
            .map(|o| o.unwrap_or(Value::Null))
            .collect();
        let result = Value::List(list);
        ctx.set_step_output(&step.id, result.clone());
        Ok(result)
    }

    pub fn evaluate_condition(
        condition: &crate::definition::Condition,
        ctx: &ExecutionContext,
    ) -> Result<bool, EngineError> {
        evaluate_condition(condition, ctx)
    }
}

/// PermissionDenied always aborts; otherwise follow [`OnError`].
fn must_abort_step(err: &EngineError, on_error: OnError) -> bool {
    err.is_permission_denied() || matches!(on_error, OnError::Abort)
}

/// Prefer a permission-denied error over a later ordinary branch failure.
fn prefer_branch_err(prev: Option<EngineError>, next: EngineError) -> EngineError {
    match prev {
        None => next,
        Some(p) if next.is_permission_denied() && !p.is_permission_denied() => next,
        Some(p) => p,
    }
}
