//! Pipeline executor for shortcuts.

use crate::control_flow::evaluate_condition;
use crate::definition::{ActionStep, IfStep, OnError, ParallelStep, RepeatStep, Shortcut, Step};
use crate::history::{ExecutionHistory, HistoryEntry};
use crate::resolver::Resolver;
use corex_core::{ActionStore, EngineError, ExecutionContext, Value};
use std::sync::Arc;
use std::time::SystemTime;
use tracing::{debug, error, info, warn};

/// Executes a [`Shortcut`] against an [`ActionStore`].
pub struct Pipeline {
    store: Arc<dyn ActionStore>,
    history: Option<ExecutionHistory>,
}

impl Pipeline {
    pub fn new(store: Arc<dyn ActionStore>) -> Self {
        Self {
            store,
            history: None,
        }
    }

    /// Enable append-only JSONL recording for each [`Self::execute`] call.
    pub fn with_history(mut self, history: ExecutionHistory) -> Self {
        self.history = Some(history);
        self
    }

    /// Execute an entire shortcut.
    pub async fn execute(
        &self,
        shortcut: &Shortcut,
        mut ctx: ExecutionContext,
    ) -> Result<Value, EngineError> {
        let started = SystemTime::now();

        // Seed variables from shortcut defaults.
        for (k, v) in &shortcut.variables {
            let resolved = Resolver::resolve_value(v, &ctx)?;
            ctx.variables.entry(k.clone()).or_insert(resolved);
        }
        // Apply input defaults.
        for decl in &shortcut.inputs {
            if !ctx.input.contains_key(&decl.name) {
                if let Some(default) = &decl.default {
                    let resolved = Resolver::resolve_value(default, &ctx)?;
                    ctx.input.insert(decl.name.clone(), resolved);
                } else if decl.required {
                    let err = EngineError::UndefinedVariable(format!(
                        "input.{}",
                        decl.name
                    ));
                    self.record_history(shortcut, started, Err(&err));
                    return Err(err);
                }
            }
        }

        info!(name = %shortcut.name, steps = shortcut.steps.len(), "开始执行快捷指令");
        let result = self
            .execute_steps(&shortcut.steps, &mut ctx, shortcut.on_error)
            .await;

        match result {
            Ok(v) => {
                info!(name = %shortcut.name, "快捷指令执行完成");
                self.record_history(shortcut, started, Ok(()));
                Ok(v)
            }
            Err(e) => {
                error!(name = %shortcut.name, error = %e, "快捷指令执行失败");
                self.record_history(shortcut, started, Err(&e));
                Err(e)
            }
        }
    }

    fn record_history(
        &self,
        shortcut: &Shortcut,
        started: SystemTime,
        outcome: Result<(), &EngineError>,
    ) {
        let Some(history) = &self.history else {
            return;
        };
        let ended = SystemTime::now();
        let result = outcome.map_err(|e| e.to_string());
        let entry = HistoryEntry::new(&shortcut.name, started, ended, result);
        history.record_best_effort(&entry);
    }

    /// Execute with per-step resilience using each step's `on_error` / `retry`.
    pub async fn execute_with_resilience(
        &self,
        shortcut: &Shortcut,
        ctx: ExecutionContext,
    ) -> Result<Value, EngineError> {
        // Same entry as execute — resilience is handled inside execute_step.
        self.execute(shortcut, ctx).await
    }

    pub async fn execute_steps(
        &self,
        steps: &[Step],
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
    ) -> Result<Value, EngineError> {
        let mut last = Value::Null;
        for step in steps {
            last = self
                .execute_step(step, ctx, default_on_error)
                .await?;
        }
        Ok(last)
    }

    pub async fn execute_step(
        &self,
        step: &Step,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
    ) -> Result<Value, EngineError> {
        match step {
            Step::Action(s) => self.run_action_step(s, ctx, default_on_error).await,
            Step::If(s) => self.run_if_step(s, ctx, default_on_error).await,
            Step::Repeat(s) => self.run_repeat_step(s, ctx, default_on_error).await,
            Step::Parallel(s) => self.run_parallel_step(s, ctx, default_on_error).await,
        }
    }

    async fn run_action_step(
        &self,
        step: &ActionStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
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
            let outcome = self.invoke_action(step, ctx).await;
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
                        warn!(
                            id = %step.id,
                            attempt,
                            error = %e,
                            "步骤失败，重试中"
                        );
                        continue;
                    }
                    return match on_error {
                        OnError::Abort => Err(e),
                        OnError::Continue | OnError::Skip => {
                            warn!(id = %step.id, error = %e, "步骤失败，按 on_error 继续");
                            ctx.set_step_output(&step.id, Value::Null);
                            Ok(Value::Null)
                        }
                    };
                }
            }
        }
    }

    async fn invoke_action(
        &self,
        step: &ActionStep,
        ctx: &mut ExecutionContext,
    ) -> Result<Value, EngineError> {
        let action = self.store.get_action(&step.action).ok_or_else(|| {
            EngineError::ActionNotRegistered(step.action.clone())
        })?;

        let params = Resolver::resolve_value(&step.params, ctx)?;
        action
            .validate(&params)
            .await
            .map_err(|e| EngineError::StepFailed {
                step: step.id.clone(),
                source: e,
            })?;

        debug!(id = %step.id, action = %step.action, "执行动作");
        action
            .execute(params, ctx)
            .await
            .map_err(|e| EngineError::StepFailed {
                step: step.id.clone(),
                source: e,
            })
    }

    async fn run_if_step(
        &self,
        step: &IfStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
    ) -> Result<Value, EngineError> {
        let pass = evaluate_condition(&step.condition, ctx)?;
        debug!(id = %step.id, pass, "求值 if 条件");
        if pass {
            Box::pin(self.execute_steps(&step.then, ctx, default_on_error)).await
        } else {
            Box::pin(self.execute_steps(&step.else_steps, ctx, default_on_error)).await
        }
    }

    async fn run_repeat_step(
        &self,
        step: &RepeatStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
    ) -> Result<Value, EngineError> {
        let mut last = Value::Null;
        if let Some(count) = step.repeat.count {
            let as_var = &step.repeat.as_var;
            for i in 0..count {
                ctx.set_variable(as_var, Value::Int(i as i64));
                last = Box::pin(self.execute_steps(&step.steps, ctx, default_on_error)).await?;
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
                last = Box::pin(self.execute_steps(&step.steps, ctx, default_on_error)).await?;
            }
        } else {
            return Err(EngineError::ControlFlow(
                "repeat 需要 count 或 each".into(),
            ));
        }
        ctx.set_step_output(&step.id, last.clone());
        Ok(last)
    }

    async fn run_parallel_step(
        &self,
        step: &ParallelStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
    ) -> Result<Value, EngineError> {
        use futures::stream::{self, StreamExt};

        let max = step
            .max_concurrency
            .unwrap_or(ctx.config.max_parallel)
            .max(1);
        let children = step.parallel.len();

        // Shared-context sequential path when concurrency is 1.
        if max <= 1 || children <= 1 {
            debug!(id = %step.id, max, children, "执行 parallel（顺序模式）");
            let mut outputs = Vec::new();
            for child in &step.parallel {
                let v = Box::pin(self.execute_step(child, ctx, default_on_error)).await?;
                outputs.push(v);
            }
            let result = Value::List(outputs);
            ctx.set_step_output(&step.id, result.clone());
            return Ok(result);
        }

        // Concurrent on the current task via buffer_unordered (no Send/JoinSet required).
        // Each branch clones context; step_outputs / variables are merged afterward.
        debug!(id = %step.id, max, children, "执行 parallel（并发 buffer_unordered）");
        let store = Arc::clone(&self.store);
        let base_ctx = ctx.clone();

        let mut results: Vec<(usize, ExecutionContext, Value)> = stream::iter(
            step.parallel
                .iter()
                .cloned()
                .enumerate()
                .map(|(idx, child)| {
                    let store = Arc::clone(&store);
                    let mut branch_ctx = base_ctx.clone();
                    async move {
                        let pipeline = Pipeline::new(store);
                        let value = Box::pin(pipeline.execute_step(
                            &child,
                            &mut branch_ctx,
                            default_on_error,
                        ))
                        .await?;
                        Ok::<_, EngineError>((idx, branch_ctx, value))
                    }
                }),
        )
        .buffer_unordered(max)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;

        results.sort_by_key(|(idx, _, _)| *idx);

        let mut outputs = Vec::with_capacity(children);
        for (_idx, branch_ctx, value) in results {
            ctx.merge_from_branch(&branch_ctx);
            outputs.push(value);
        }

        let result = Value::List(outputs);
        ctx.set_step_output(&step.id, result.clone());
        Ok(result)
    }

    /// Public helper mirroring control_flow for callers that have a Pipeline.
    pub fn evaluate_condition(
        condition: &crate::definition::Condition,
        ctx: &ExecutionContext,
    ) -> Result<bool, EngineError> {
        evaluate_condition(condition, ctx)
    }
}
