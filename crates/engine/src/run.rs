//! 触发器与 CLI 共用的指令执行逻辑。

use crate::audit::ExecutionAudit;
use crate::definition::Directive;
use crate::history::{ExecutionHistory, HistorySink};
use crate::pipeline::Pipeline;
use corex_core::{ActionStore, EngineError, ExecutionContext, RuntimeConfig, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::debug;

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
    /// 执行历史的落点；没注入时按配置落到 JSONL（老行为）。
    history: Option<Arc<dyn HistorySink>>,
    /// 按名字取指令的口；没注入时只能跑文件。
    source: Option<Arc<dyn DirectiveSource>>,
}

impl DirectiveRunner {
    pub fn new(store: Arc<dyn ActionStore>, runtime: RuntimeConfig, data_dir: PathBuf) -> Self {
        Self {
            store,
            runtime,
            data_dir,
            history: None,
            source: None,
        }
    }

    /// 换掉执行历史的落点：daemon / CLI / MCP 都传指令库那一份，执行日志才与指令同库。
    pub fn with_history(mut self, history: Arc<dyn HistorySink>) -> Self {
        self.history = Some(history);
        self
    }

    /// 注入「按名字取指令」的口。
    pub fn with_source(mut self, source: Arc<dyn DirectiveSource>) -> Self {
        self.source = Some(source);
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
        if let Some(history) = self.find_history() {
            pipeline = pipeline.with_history(history);
        }
        let audit_path = self.data_dir.join("audit.jsonl");
        if let Ok(audit) = ExecutionAudit::open(audit_path) {
            pipeline = pipeline.with_audit(audit);
        }
        pipeline.execute(directive, ctx).await
    }

    /// 历史落点：显式注入的优先，否则按 `[history]` 配置开一份 JSONL。
    fn find_history(&self) -> Option<Arc<dyn HistorySink>> {
        if let Some(history) = &self.history {
            return Some(Arc::clone(history));
        }
        if !self.runtime.history.enabled {
            return None;
        }
        let path = if self.runtime.history.file.is_absolute() {
            self.runtime.history.file.clone()
        } else {
            self.data_dir.join(&self.runtime.history.file)
        };
        ExecutionHistory::open(path)
            .ok()
            .map(|history| Arc::new(history) as Arc<dyn HistorySink>)
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
    let mut runner = DirectiveRunner::new(store, runtime, data_dir.to_path_buf());
    if let Some(history) = history {
        runner = runner.with_history(Arc::clone(history));
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
