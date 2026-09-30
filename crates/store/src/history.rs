//! 把指令库接到引擎的两个口上：按名字取指令（[`DirectiveSource`]）与记执行日志（[`HistorySink`]）。
//!
//! 引擎不能依赖库这一层（依赖方向是 `store → engine`），所以接口定义在引擎侧、实现放在这里。
//! daemon / CLI / MCP 拿到的就是这两个值：指令、执行日志、上次执行时间从此都只有库里一处真相。

use crate::store::{DirectiveStore, RUNS_SCAN};
use corex_core::EngineError;
use corex_engine::{Directive, DirectiveHistory, DirectiveSource, HistoryEntry, HistorySink};
use std::collections::BTreeMap;
use std::sync::Arc;
use tracing::warn;
/// 指令库里的执行日志。
#[derive(Debug, Clone)]
pub struct SqliteHistory {
    store: Arc<DirectiveStore>,
}

impl SqliteHistory {
    pub fn new(store: Arc<DirectiveStore>) -> Self {
        Self { store }
    }
}

impl HistorySink for SqliteHistory {
    fn record_best_effort(&self, entry: &HistoryEntry) {
        if let Err(error) = self.store.append_run(entry) {
            // 记不下来不该让一次已经跑完的指令报失败，与 JSONL 时代的选择一致。
            warn!(directive = %entry.directive, error = %error, "写入执行日志失败");
        }
    }

    fn recent(&self, name: Option<&str>, limit: usize) -> Vec<HistoryEntry> {
        self.store.recent_runs(name, limit).unwrap_or_else(|error| {
            warn!(error = %error, "读取执行日志失败");
            Vec::new()
        })
    }

    fn recent_names(&self, limit: usize) -> Vec<String> {
        self.store.recent_run_names(limit).unwrap_or_else(|error| {
            warn!(error = %error, "读取执行日志失败");
            Vec::new()
        })
    }

    fn by_directive(&self) -> BTreeMap<String, DirectiveHistory> {
        self.store
            .runs_by_directive(RUNS_SCAN)
            .unwrap_or_else(|error| {
                warn!(error = %error, "读取执行日志失败");
                BTreeMap::new()
            })
    }
}

/// 按名字从库里取指令。
#[derive(Debug, Clone)]
pub struct StoreDirectiveSource {
    store: Arc<DirectiveStore>,
}

impl StoreDirectiveSource {
    pub fn new(store: Arc<DirectiveStore>) -> Self {
        Self { store }
    }
}

impl DirectiveSource for StoreDirectiveSource {
    fn load(&self, name: &str) -> Result<Directive, EngineError> {
        let record = self.store.fetch(name).map_err(EngineError::from)?;
        Ok(record.definition)
    }
}
