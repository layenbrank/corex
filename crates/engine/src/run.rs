//! 触发器与 CLI 共用的指令执行逻辑。

use crate::audit::ExecutionAudit;
use crate::definition::Directive;
use crate::history::HistorySink;
use crate::pipeline::Pipeline;
use crate::supervisor::progress::FileProgress;
use crate::supervisor::JobKind;
use corex_core::{ActionStore, EngineError, ExecutionContext, Observer, RuntimeConfig, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, warn};

/// 一条指令从哪来。
///
/// v13 起指令的真相在指令库（SQLite），而引擎不能依赖库那一层（依赖方向是 `store → engine`），
/// 所以由调用方把「按名字取模型」注入进来。
///
/// watch / cron 尤其需要它：它们过去每次触发都读一遍 `directive_path`。指令进库之后那个路径
/// 可能已经不存在，而旧文件还在磁盘上的话更糟 —— 用户改了库里的那份，触发器照旧跑文件里的
/// 另一份，正是「改了没生效」这一类最难查的毛病。
pub trait DirectiveSource: Send + Sync + std::fmt::Debug {
    /// 按名字取模型；库里没有时给 [`EngineError::DirectiveNotFound`]。
    fn load(&self, name: &str) -> Result<Directive, EngineError>;
}

/// supervisor 跑作业时要接上的两个口。
///
/// 收成一个值而不是两个并列参数：watch 与 cron 的 supervisor 构造点已经够长，而且这两件事
/// 总是要么都有、要么都没有（有指令库就都接库，老用法就都不接）。
#[derive(Clone, Default)]
pub struct SupervisorIo {
    /// 每次触发从哪取指令；`None` 时按 `directive_path` 读文件。
    pub source: Option<Arc<dyn DirectiveSource>>,
    /// 执行日志写到哪；`None` 时由 runner 按 `[history]` 配置决定。
    pub history: Option<Arc<dyn HistorySink>>,
}

/// 用与 CLI 相同的流水线接线方式运行指令。
pub struct DirectiveRunner {
    pub store: Arc<dyn ActionStore>,
    pub runtime: RuntimeConfig,
    pub data_dir: PathBuf,
    /// 执行历史的落点；`None` = 不记。
    ///
    /// **落点由注入决定，不再看配置**：`[history].file` 在 v13 里只剩「旧 JSONL 账本的位置」
    /// 这一个用途（首次打开指令库时导入一次）。CLI / daemon / MCP 注入的都是指令库那一份，
    /// 所以执行日志与指令同库；嵌入方不想引 SQLite 时可以自己注入
    /// [`crate::ExecutionHistory`]。
    history: Option<Arc<dyn HistorySink>>,
    /// 按名字取指令的口；没注入时只能跑文件。
    source: Option<Arc<dyn DirectiveSource>>,
    /// 进度上报口；cron / watch 触发时挂上落盘 Observer。
    observer: Option<Arc<dyn Observer>>,
}

impl DirectiveRunner {
    pub fn new(store: Arc<dyn ActionStore>, runtime: RuntimeConfig, data_dir: PathBuf) -> Self {
        Self {
            store,
            runtime,
            data_dir,
            history: None,
            source: None,
            observer: None,
        }
    }

    /// 执行历史的落点：CLI / daemon / MCP 都传指令库那一份，执行日志才与指令同库。
    /// 不传就是**不记**（`[history].enabled` 由调用方决定要不要注入）。
    pub fn with_history(mut self, history: Arc<dyn HistorySink>) -> Self {
        self.history = Some(history);
        self
    }

    /// 注入「按名字取指令」的口。
    pub fn with_source(mut self, source: Arc<dyn DirectiveSource>) -> Self {
        self.source = Some(source);
        self
    }

    /// 挂上进度上报（守护触发落盘 / IPC stream）。
    pub fn with_observer(mut self, observer: Arc<dyn Observer>) -> Self {
        self.observer = Some(observer);
        self
    }

    /// 按名字跑指令库里的指令。
    pub async fn run_named(
        &self,
        name: &str,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, EngineError> {
        let source = self
            .source
            .clone()
            .ok_or_else(|| EngineError::Usage("这次运行没有接上指令库".into()))?;
        let directive = source.load(name)?;
        self.run(&directive, inputs).await
    }

    pub async fn run_file(
        &self,
        path: &Path,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, EngineError> {
        let directive = Directive::from_yaml_file(path)?;
        self.run(&directive, inputs).await
    }

    pub async fn run(
        &self,
        directive: &Directive,
        inputs: HashMap<String, Value>,
    ) -> Result<Value, EngineError> {
        let ctx = ExecutionContext::new(self.runtime.clone()).with_input(inputs);
        let mut pipeline = Pipeline::new(Arc::clone(&self.store));
        if let Some(history) = &self.history {
            pipeline = pipeline.with_history(Arc::clone(history));
        }
        if let Some(observer) = &self.observer {
            pipeline = pipeline.with_observer(Arc::clone(observer));
        }
        let audit_path = self.data_dir.join("audit.jsonl");
        if let Ok(audit) = ExecutionAudit::open(audit_path) {
            pipeline = pipeline.with_audit(audit);
        }
        pipeline.execute(directive, ctx).await
    }
}

/// 触发器里跑一条作业：库里有同名指令就用库里那份，否则退回文件。
///
/// 回退只为「拿一个 YAML 路径去 watch / cron」这类用法留着；库里有同名指令时一律以库里
/// 为准，不然用户改的是库里那份、跑的还是磁盘上那份。
pub async fn run_directive_spec(
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    data_dir: &Path,
    source: Option<&Arc<dyn DirectiveSource>>,
    history: Option<&Arc<dyn HistorySink>>,
    name: &str,
    path: &Path,
) -> Result<Value, EngineError> {
    run_directive_spec_with_observer(store, runtime, data_dir, source, history, name, path, None)
        .await
}

/// 同 [`run_directive_spec`]，可挂进度 Observer。
pub async fn run_directive_spec_with_observer(
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    data_dir: &Path,
    source: Option<&Arc<dyn DirectiveSource>>,
    history: Option<&Arc<dyn HistorySink>>,
    name: &str,
    path: &Path,
    observer: Option<Arc<dyn Observer>>,
) -> Result<Value, EngineError> {
    let mut runner = DirectiveRunner::new(store, runtime, data_dir.to_path_buf());
    if let Some(history) = history {
        runner = runner.with_history(Arc::clone(history));
    }
    if let Some(observer) = observer {
        runner = runner.with_observer(observer);
    }

    let Some(source) = source else {
        return runner.run_file(path, HashMap::new()).await;
    };
    match source.load(name) {
        Ok(directive) => runner.run(&directive, HashMap::new()).await,
        Err(EngineError::DirectiveNotFound(_)) => {
            debug!(name, path = %path.display(), "库里没有这条指令，按文件跑");
            runner.run_file(path, HashMap::new()).await
        }
        Err(error) => Err(error),
    }
}

/// 守护触发：落盘进度后再跑。`kind` + 指令名对应 `<data>/<kind>/<name>/`。
pub async fn run_supervised_directive(
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    data_dir: &Path,
    source: Option<&Arc<dyn DirectiveSource>>,
    history: Option<&Arc<dyn HistorySink>>,
    kind: JobKind,
    name: &str,
    path: &Path,
) -> Result<Value, EngineError> {
    let job_dir = crate::supervisor::JobMeta::job_dir(data_dir, kind, name);
    let progress = match FileProgress::begin_run(&job_dir, kind, name) {
        Ok(p) => Some(Arc::new(p)),
        Err(error) => {
            warn!(
                directive = %name,
                kind = kind.as_str(),
                error = %error,
                "无法打开进度文件，本次触发不落盘进度"
            );
            None
        }
    };
    let observer = progress.clone().map(|p| p as Arc<dyn Observer>);
    let result = run_directive_spec_with_observer(
        store,
        runtime,
        data_dir,
        source,
        history,
        name,
        path,
        observer,
    )
    .await;
    if let Some(progress) = progress {
        match &result {
            Ok(_) => progress.finish(true, None),
            Err(error) => progress.finish(false, Some(error.to_string())),
        }
    }
    result
}

/// 给触发器 supervisor 用的便捷包装。
pub async fn run_directive_file(
    store: Arc<dyn ActionStore>,
    runtime: RuntimeConfig,
    data_dir: PathBuf,
    path: &Path,
) -> Result<Value, EngineError> {
    DirectiveRunner::new(store, runtime, data_dir)
        .run_file(path, HashMap::new())
        .await
}
