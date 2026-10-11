//! 执行进度的帧格式与它的两个落点。
//!
//! 这里只做三件事：把 [`corex_core::progress::Observer`] 的四次回调变成能过线的
//! [`ProgressEvent`]，再补一个不属于任何步骤的 [`ProgressEvent::Heartbeat`]（它证明
//! “对面没死”，见该变体），最后给它们准备两个落点——服务端的 [`Outlet`]（写回连接）
//! 与客户端的 [`Replay`]（重放进上报口）。
//!
//! **两边共用一套词汇**：`corex run --json-events` 打出的每一行就是 [`ProgressEvent`]
//! 的 JSON。宿主因此不必为「本地」与「远程」记两套字段名，CLI 的渲染器也能在两条路径上
//! 原样复用。

use crate::protocol::Response;
use corex_core::{Mark, Observer, Spot, Stream, Unit};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// 步骤帧上的可选层级信息（编排器树映射）；旧客户端可忽略。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressScope {
    /// 从根到当前步骤的 id 路径，如 `["fanout", "copy"]`。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub path: Vec<String>,
    /// 并行分支下标（若有）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_index: Option<u32>,
    /// 本次执行 id（若宿主/守护已分配）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
}

/// 一次进度的中间帧。
///
/// 与 [`Observer`] 的四个方法一一对应，因此任何一侧都能无损地重放它。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProgressEvent {
    /// 动作步骤开始。`seq` 从 1 开始递增，宿主据此给步骤编号。
    StepStart {
        seq: u64,
        step: String,
        action: String,
        #[serde(flatten)]
        scope: ProgressScope,
    },
    /// 步骤内的分块进度（大文件复制、目录删除…）。`total` 未知时为 `null`。
    StepProgress {
        step: String,
        action: String,
        done: u64,
        total: Option<u64>,
        unit: Unit,
        #[serde(flatten)]
        scope: ProgressScope,
    },
    /// 动作吐出来的一段文本（子进程 stdout / stderr）。
    ///
    /// 与 `step_progress` 一样是**增量**：`text` 按到达顺序拼接才是完整输出，它可能含多行、
    /// 也可能在半行处断开（切点由动作的读缓冲决定）。宿主不该把它当「一行」。
    StepOutput {
        step: String,
        action: String,
        stream: Stream,
        text: String,
        #[serde(flatten)]
        scope: ProgressScope,
    },
    /// 动作步骤结束。`took_ms` 是整步（含重试）的墙钟耗时。
    StepEnd {
        step: String,
        action: String,
        took_ms: u64,
        ok: bool,
        #[serde(flatten)]
        scope: ProgressScope,
    },
    /// 执行请求的**心跳**：从请求到达到它跑完，每两秒一帧。
    Heartbeat {
        /// 还在队列里等执行名额（`false` = 已经在跑了）
        is_queued: bool,
        /// 从请求到此刻等了多久：排队与执行都算在内
        waited_ms: u64,
    },
}

impl ProgressEvent {
    /// 把这一帧重放进一个上报口。
    pub fn replay(&self, observer: &dyn Observer) {
        match self {
            Self::StepStart { step, action, .. } => observer.begin(Spot { id: step, action }),
            Self::StepProgress {
                step,
                action,
                done,
                total,
                unit,
                ..
            } => observer.chunk(
                Spot { id: step, action },
                Mark {
                    done: *done,
                    total: *total,
                    unit: *unit,
                },
            ),
            Self::StepOutput {
                step,
                action,
                stream,
                text,
                ..
            } => observer.output(Spot { id: step, action }, *stream, text),
            Self::StepEnd {
                step,
                action,
                took_ms,
                ok,
                ..
            } => observer.end(
                Spot { id: step, action },
                Duration::from_millis(*took_ms),
                *ok,
            ),
            Self::Heartbeat { .. } => {}
        }
    }

    /// 叶子步骤的默认 scope：`path = [step]`。
    pub fn leaf_scope(step: &str) -> ProgressScope {
        ProgressScope {
            path: vec![step.to_string()],
            branch_index: None,
            run_id: None,
        }
    }
}

/// 中间帧的落点。
pub trait FrameSink: Send + Sync {
    fn frame(&self, progress: &ProgressEvent);
}

/// 服务端：把帧写回发起这条请求的连接。
#[derive(Debug, Clone)]
pub struct Outlet {
    id: u64,
    tx: mpsc::Sender<Response>,
}

impl Outlet {
    pub fn new(id: u64, tx: mpsc::Sender<Response>) -> Self {
        Self { id, tx }
    }

    pub fn offer(&self, progress: ProgressEvent) -> bool {
        self.tx
            .try_send(Response::Event {
                id: self.id,
                progress,
            })
            .is_ok()
    }
}

impl FrameSink for Outlet {
    fn frame(&self, progress: &ProgressEvent) {
        let _ = self.offer(progress.clone());
    }
}

/// 客户端：把帧重放进本地的上报口。
#[derive(Debug, Clone)]
pub struct Replay {
    observer: Option<Arc<dyn Observer>>,
}

impl Replay {
    pub fn new(observer: Option<Arc<dyn Observer>>) -> Self {
        Self { observer }
    }
}

impl FrameSink for Replay {
    fn frame(&self, progress: &ProgressEvent) {
        if let Some(observer) = &self.observer {
            progress.replay(&**observer);
        }
    }
}
