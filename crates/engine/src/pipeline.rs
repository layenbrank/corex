//! 指令的流水线执行器。

use crate::audit::{self, AuditEntry, ExecutionAudit};
use crate::control_flow::evaluate_condition;
use crate::definition::{
    ActionStep, Directive, IfStep, OnError, ParallelStep, Permissions, RepeatStep, Step, StepsStep,
};
use crate::history::{ExecutionHistory, HistoryEntry};
use crate::inputs::fill_input_defaults;
use crate::resolver::Resolver;
use corex_core::{ActionError, ActionStore, EngineError, ExecutionContext, Observer, Spot, Value};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};
use tracing::{debug, error, info, warn};

/// 针对一个 [`ActionStore`] 执行 [`Directive`]。
pub struct Pipeline {
    store: Arc<dyn ActionStore>,
    history: Option<ExecutionHistory>,
    audit: Option<ExecutionAudit>,
    /// 步骤进度的上报口；`None` 时引擎只走日志。
    observer: Option<Arc<dyn Observer>>,
    /// 在 [`Self::execute`] 期间设置，用于步骤审计 / 日志。
    run_name: Option<String>,
}

impl Pipeline {
    pub fn new(store: Arc<dyn ActionStore>) -> Self {
        Self {
            store,
            history: None,
            audit: None,
            observer: None,
            run_name: None,
        }
    }

    /// 为每次 [`Self::execute`] 开启只追加的 JSONL 记录。
    pub fn with_history(mut self, history: ExecutionHistory) -> Self {
        self.history = Some(history);
        self
    }

    /// 开启步骤级脱敏审计 JSONL。
    pub fn with_audit(mut self, audit: ExecutionAudit) -> Self {
        self.audit = Some(audit);
        self
    }

    /// 接收步骤进度。
    ///
    /// 上报口会同时放进 [`ExecutionContext`]，因此长耗时的动作可以自己补充分块进度
    /// （见 [`ExecutionContext::chunk`]）。不给就是不上报，引擎零开销。
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// 派生一份共享同一套依赖（store / 审计 / 历史 / 上报口）的流水线，只换运行名。
    ///
    /// 「同一套依赖、另一个运行名」只有这一条路径：[`Self::execute`] 换的是指令名。
    fn fork(&self, run_name: Option<String>) -> Self {
        Self {
            store: Arc::clone(&self.store),
            history: self.history.clone(),
            audit: self.audit.clone(),
            observer: self.observer.clone(),
            run_name,
        }
    }

    /// 执行整条指令。
    pub async fn execute(
        &self,
        directive: &Directive,
        mut ctx: ExecutionContext,
    ) -> Result<Value, EngineError> {
        let started = SystemTime::now();
        let pipeline = self.fork(Some(directive.name.clone()));
        // 上报口从流水线转交给上下文，于是动作不必自己去拿它。
        ctx.observer = pipeline.observer.clone();

        if let Err(e) = Resolver::seed_variables(&directive.variables, &mut ctx) {
            pipeline.record_history(directive, started, Err(&e));
            return Err(e);
        }
        if let Err(e) = fill_input_defaults(directive, &mut ctx) {
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
            // `Box::pin`：顺序块回到 `execute_steps`，与上面的 `execute_step` 互为递归，
            // 不加间接就是无限大的 future（`parallel` 里同一处也这么写）。
            Step::Steps(s) => {
                Box::pin(self.execute_steps(&s.steps, ctx, default_on_error, permissions)).await
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
        if let Some(cond) = &step.when
            && !evaluate_condition(cond, ctx)?
        {
            debug!(id = %step.id, "when 条件为假，跳过步骤");
            return Ok(Value::Null);
        }

        // 上报按**整步**计（含重试），与审计里的 duration 口径一致。
        let at = Spot {
            id: &step.id,
            action: &step.action,
        };
        if let Some(observer) = &self.observer {
            observer.begin(at);
        }
        ctx.enter_step(&step.id, &step.action);
        let t0 = Instant::now();
        let outcome = self
            .attempts(step, ctx, default_on_error, permissions)
            .await;
        ctx.leave_step();
        if let Some(observer) = &self.observer {
            observer.end(at, t0.elapsed(), outcome.is_ok());
        }
        outcome
    }

    /// 调动作直到成功、重试耗尽或 `on_error` 给出结论。
    async fn attempts(
        &self,
        step: &ActionStep,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Value, EngineError> {
        let on_error = step.on_error.unwrap_or(default_on_error);
        let retries = step.retry.unwrap_or(0);
        let mut n = 0u32;

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
                    if n < retries {
                        n += 1;
                        warn!(id = %step.id, attempt = n, error = %e, "步骤失败，重试中");
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

        if let Err(e) = permissions.allows_action(&*self.store, &step.action) {
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

        // 原样参数的动作自己做模板解释，跳过预解析。
        let params = if action.is_raw_params() {
            step.params.clone()
        } else {
            match Resolver::resolve_value(&step.params, ctx) {
                Ok(p) => p,
                Err(e) => {
                    self.record_step_audit(step, t0.elapsed().as_millis() as u64, Err(&e));
                    return Err(e);
                }
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
        let timeout = ctx.config.step_timeout;
        let fut = action.execute(params, ctx);
        let result = if timeout > 0 {
            match tokio::time::timeout(Duration::from_secs(timeout), fut).await {
                Ok(r) => r,
                Err(_) => Err(ActionError::Timeout(format!(
                    "步骤 {} 超过 {timeout}s",
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
        // 先把「第几轮、绑什么值」摊平成一个序列，串行与并发两条路共用。
        let items: Vec<Value> = if let Some(count) = step.repeat.count {
            (0..count).map(|i| Value::Int(i as i64)).collect()
        } else if let Some(each) = &step.repeat.each {
            match Resolver::resolve_string(each, ctx)? {
                Value::Array(list) => list,
                other => {
                    return Err(EngineError::ControlFlow(format!(
                        "repeat.each 必须解析为列表，得到: {other}"
                    )));
                }
            }
        } else {
            return Err(EngineError::ControlFlow("repeat 需要 count 或 each".into()));
        };

        // `max_concurrency` 与 `repeat` 同级：省略或 `1` 是串行，`> 1` 是并发。
        let concurrency = step.max_concurrency.unwrap_or(1).max(1);
        let last = if concurrency > 1 {
            // 并发：每个元素一份上下文副本，跑完按元素顺序合并回来。
            // 这里的循环体是引擎自己合成的，不带 id。
            let body = Step::Steps(StepsStep {
                id: String::new(),
                steps: step.steps.clone(),
            });
            let values = self
                .run_fanout(
                    Fanout {
                        id: &step.id,
                        count: items.len(),
                        max_concurrency: Some(concurrency),
                    },
                    |_| &body,
                    |idx, branch_ctx| {
                        bind_loop_item(
                            &step.repeat.as_var,
                            &step.repeat.index_var,
                            idx,
                            items[idx].clone(),
                            branch_ctx,
                        );
                    },
                    ctx,
                    default_on_error,
                    permissions,
                )
                .await?;
            values.into_iter().next_back().unwrap_or(Value::Null)
        } else {
            // 串行：元素共享一份上下文，前一个元素写下的变量后一个看得见。
            let mut last = Value::Null;
            for (idx, item) in items.into_iter().enumerate() {
                bind_loop_item(&step.repeat.as_var, &step.repeat.index_var, idx, item, ctx);
                last =
                    Box::pin(self.execute_steps(&step.steps, ctx, default_on_error, permissions))
                        .await?;
            }
            last
        };
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
        let values = self
            .run_fanout(
                Fanout {
                    id: &step.id,
                    count: step.parallel.len(),
                    max_concurrency: step.max_concurrency,
                },
                |idx| &step.parallel[idx],
                |_, _| {},
                ctx,
                default_on_error,
                permissions,
            )
            .await?;
        let result = Value::Array(values);
        ctx.set_step_output(&step.id, result.clone());
        Ok(result)
    }

    /// 并发跑 `count` 个分支，按序号把每个分支的上下文合并回来；返回值与序号对齐。
    ///
    /// `parallel` 的分支与 `repeat` 的并发元素共用这一段：差别只有「第 i 个分支是谁」
    /// （`branch_of`）与「进分支前先绑什么变量」（`bind`）。
    async fn run_fanout<'b>(
        &self,
        fanout: Fanout<'_>,
        branch_of: impl Fn(usize) -> &'b Step + Sync,
        bind: impl Fn(usize, &mut ExecutionContext) + Sync,
        ctx: &mut ExecutionContext,
        default_on_error: OnError,
        permissions: &Permissions,
    ) -> Result<Vec<Value>, EngineError> {
        use futures::stream::{self, StreamExt};

        let Fanout {
            id,
            count,
            max_concurrency,
        } = fanout;
        let max = max_concurrency.unwrap_or(ctx.config.max_parallel).max(1);
        debug!(id, max, count, "并发跑分支（buffer_unordered）");
        let base_ctx = ctx.clone();
        let perms = permissions.clone();

        let collected: Vec<Result<(usize, ExecutionContext, Value), EngineError>> =
            stream::iter((0..count).map(|idx| {
                // 借而不是克隆：`repeat.each` 的循环体只有一份，不能按元素各复制一遍。
                let child = branch_of(idx);
                let mut branch_ctx = base_ctx.clone();
                bind(idx, &mut branch_ctx);
                let perms = perms.clone();
                async move {
                    // 分支只读地复用父流水线：store / 审计 / 历史 / 上报口都在它身上，
                    // 再拼一个新 Pipeline 只会多一遍克隆。
                    let value = Box::pin(self.execute_step(
                        child,
                        &mut branch_ctx,
                        default_on_error,
                        &perms,
                    ))
                    .await?;
                    Ok::<_, EngineError>((idx, branch_ctx, value))
                }
            }))
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
        // 按序号合并：并发跑，合并顺序仍然确定，`save_to` 的「后者覆盖前者」才可预期。
        successes.sort_by_key(|(idx, _, _)| *idx);

        if let Some(e) = branch_err {
            if must_abort_step(&e, default_on_error) {
                return Err(e);
            }
            warn!(id, error = %e, "部分分支失败，按 on_error 继续");
        }

        let mut outputs: Vec<Option<Value>> = vec![None; count];
        for (idx, branch_ctx, value) in successes {
            ctx.merge_from_branch(&branch_ctx);
            if idx < outputs.len() {
                outputs[idx] = Some(value);
            }
        }
        Ok(outputs
            .into_iter()
            .map(|o| o.unwrap_or(Value::Null))
            .collect())
    }

    pub fn evaluate_condition(
        condition: &crate::definition::Condition,
        ctx: &ExecutionContext,
    ) -> Result<bool, EngineError> {
        evaluate_condition(condition, ctx)
    }
}

/// 进循环体前绑好 `as` / `index` 两个变量；串行与并发两条路共用，免得两处写法漂移。
fn bind_loop_item(
    as_var: &str,
    index_var: &str,
    idx: usize,
    item: Value,
    ctx: &mut ExecutionContext,
) {
    ctx.set_variable(as_var, item);
    ctx.set_variable(index_var, Value::Int(idx as i64));
}

/// 一次扇出：谁在扇、扇多少个、最多几个同时在跑。
///
/// 捆在一起是为了让 [`Pipeline::run_fanout`] 的签名留得下两个闭包。
struct Fanout<'a> {
    id: &'a str,
    count: usize,
    max_concurrency: Option<usize>,
}

/// PermissionDenied 一律中止；其余按 [`OnError`] 处理。
fn must_abort_step(err: &EngineError, on_error: OnError) -> bool {
    err.is_permission_denied() || matches!(on_error, OnError::Abort)
}

/// 优先报告权限拒绝，而不是之后某个普通分支失败。
fn prefer_branch_err(prev: Option<EngineError>, next: EngineError) -> EngineError {
    match prev {
        None => next,
        Some(p) if next.is_permission_denied() && !p.is_permission_denied() => next,
        Some(p) => p,
    }
}
