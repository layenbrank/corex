//! 执行进度的上报口。
//!
//! 引擎只在**动作步骤**前后上报；长耗时的动作（大文件复制、目录删除、HTTP 下载）
//! 可以在过程中调用 [`ExecutionContext::chunk`] 自己补充分块进度。
//!
//! 这个模块刻意不依赖任何终端概念：怎么画（spinner / 逐行文本 / 干脆不画）是调用方的事，
//! 引擎只负责在正确的位置把事实说出来。守护进程、CLI、嵌入式宿主因此共用同一套事实源。

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// 步骤在指令里的位置。借自正在执行的指令，不复制字符串。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spot<'a> {
    /// 步骤 id
    pub id: &'a str,
    /// 动作 id
    pub action: &'a str,
}

/// 一次分块进度的快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Mark {
    /// 已完成的工作量
    pub done: u64,
    /// 预期总量；未知时为 `None`
    pub total: Option<u64>,
    /// `done` / `total` 的单位。上报口只管展示，不负责换算。
    pub unit: Unit,
}

/// [`Mark`] 里数量的单位。
///
/// 它会过 IPC（`corex_ipc::ProgressEvent`），所以这里是线上契约的一部分：
/// 两个变体的名字即 JSON 里的 `"bytes"` / `"items"`，改名就是协议变更。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    /// 字节（复制、下载）
    Bytes,
    /// 条目数（递归删除）
    Items,
}

/// 执行进度的上报口。
///
/// 实现必须线程安全：`parallel` 步骤会在并发任务里上报。
/// 所有方法都有空实现，因此只关心其中一两个事件的实现者不必写一堆空方法。
pub trait Observer: Send + Sync + std::fmt::Debug {
    /// 动作步骤开始。
    fn begin(&self, _at: Spot<'_>) {}

    /// 步骤内的分块进度。动作每完成一个分块调用一次，调用点决定粒度。
    fn chunk(&self, _at: Spot<'_>, _mark: Mark) {}

    /// 动作步骤结束。`took` 是整步的墙钟耗时，`ok` 为假表示这一步以失败告终。
    fn end(&self, _at: Spot<'_>, _took: Duration, _ok: bool) {}
}

/// 可持有、可跨线程的上报句柄。
///
/// 阻塞任务里的动作借不到 [`ExecutionContext`]——它只在执行栈上——而长时间没动静的
/// 恰恰是那些地方（模板匹配、OCR）。[`reporter`] 把上报口与当前步骤复制成一个可 `Send`
/// 的句柄，之后在哪个线程上报都行。
///
/// [`ExecutionContext`]: crate::ExecutionContext
/// [`reporter`]: crate::ExecutionContext::reporter
#[derive(Debug, Clone)]
pub struct Reporter {
    observer: Arc<dyn Observer>,
    id: String,
    action: String,
}

impl Reporter {
    /// 只由 [`ExecutionContext`] 构造：拿到的句柄一定对应一个真实存在的步骤。
    ///
    /// [`ExecutionContext`]: crate::ExecutionContext
    pub(crate) fn new(observer: Arc<dyn Observer>, at: &Owned) -> Self {
        Self {
            observer,
            id: at.id.clone(),
            action: at.action.clone(),
        }
    }

    /// 上报一块进度，语义与 [`ExecutionContext::chunk`] 相同。
    ///
    /// [`ExecutionContext::chunk`]: crate::ExecutionContext::chunk
    pub fn chunk(&self, done: u64, total: Option<u64>, unit: Unit) {
        self.observer.chunk(
            Spot {
                id: &self.id,
                action: &self.action,
            },
            Mark { done, total, unit },
        );
    }
}

/// [`Spot`] 的持有形式：引擎在调用动作前把它放进 [`ExecutionContext`]，
/// 动作据此不必自己重复动作 id。
///
/// [`ExecutionContext`]: crate::ExecutionContext
#[derive(Debug, Clone)]
pub(crate) struct Owned {
    pub(crate) id: String,
    pub(crate) action: String,
}
