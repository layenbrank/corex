//! 触发器与 CLI 共用的指令执行逻辑。

use crate::audit::ExecutionAudit;
use crate::definition::Directive;
use crate::history::ExecutionHistory;
use crate::pipeline::Pipeline;
use corex_core::{ActionStore, EngineError, ExecutionContext, RuntimeConfig, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// 用与 CLI 相同的流水线接线方式运行指令。
pub struct DirectiveRunner {
    pub store: Arc<dyn ActionStore>,
    pub runtime: RuntimeConfig,
    pub data_dir: PathBuf,
}

impl DirectiveRunner {
    pub fn new(store: Arc<dyn ActionStore>, runtime: RuntimeConfig, data_dir: PathBuf) -> Self {
        Self {
            store,
            runtime,
            data_dir,
        }
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
        if self.runtime.history.enabled {
            let hist_path = if self.runtime.history.file.is_absolute() {
                self.runtime.history.file.clone()
            } else {
                self.data_dir.join(&self.runtime.history.file)
            };
            if let Ok(history) = ExecutionHistory::open(hist_path) {
                pipeline = pipeline.with_history(history);
            }
        }
        let audit_path = self.data_dir.join("audit.jsonl");
        if let Ok(audit) = ExecutionAudit::open(audit_path) {
            pipeline = pipeline.with_audit(audit);
        }
        pipeline.execute(directive, ctx).await
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
