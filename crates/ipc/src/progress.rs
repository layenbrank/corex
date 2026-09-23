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
    },
    /// 步骤内的分块进度（大文件复制、目录删除…）。`total` 未知时为 `null`。
    StepProgress {
        step: String,
        action: String,
        done: u64,
        total: Option<u64>,
        unit: Unit,
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
    },
    /// 动作步骤结束。`took_ms` 是整步（含重试）的墙钟耗时。
    StepEnd {
        step: String,
        action: String,
        took_ms: u64,
        ok: bool,
    },
    /// 执行请求的**心跳**：从请求到达到它跑完，每两秒一帧。
    ///
    /// 它不对应任何一步，只解决一件事：**排队与卡死在客户端看来是一样的**——两者都是
    /// 「一段时间没有任何帧」，而一段几分钟的排队足够撞穿宿主的请求时限（超时被报成失败，
    /// 请求其实还排在队列里，之后照样执行）。有心跳之后，「多久没有帧」才等于「对面是不是死了」。
    Heartbeat {
        /// 还在队列里等执行名额（`false` = 已经在跑了）
        is_queued: bool,
        /// 从请求到此刻等了多久：排队与执行都算在内
        waited_ms: u64,
    },
}

impl ProgressEvent {
    /// 把这一帧重放进一个上报口。
    ///
    /// 收到 `Response::Event` 的一方不必自己去拼 `Spot` / `Mark`——CLI 的 `Steps` /
    /// `Events` 两个渲染器正是靠它同时服务于本地执行与远程执行。
    pub fn replay(&self, observer: &dyn Observer) {
        match self {
            Self::StepStart { step, action, .. } => observer.begin(Spot { id: step, action }),
            Self::StepProgress {
                step,
                action,
                done,
                total,
                unit,
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
            } => observer.output(Spot { id: step, action }, *stream, text),
            Self::StepEnd {
                step,
                action,
                took_ms,
                ok,
            } => observer.end(
                Spot { id: step, action },
                Duration::from_millis(*took_ms),
                *ok,
            ),
            // 心跳不属于任何一步：它服务的是「连接的另一头别把我当卡死」，不是渲染器。
            // 上报口没有它的位置，所以直接丢掉。
            Self::Heartbeat { .. } => {}
        }
    }
}

/// 中间帧的落点。
///
/// 两个方向的实现共用这一个形状，因为它们对帧的态度完全一致——**尽力而为，绝不阻塞执行**：
/// 服务端队列满就丢帧（不该让一条渲染慢的客户端把指令执行拖住），客户端直接把帧喂给渲染器。
pub trait FrameSink: Send + Sync {
    /// 收下一帧。实现必须立刻返回：它跑在连接的读写路径上。
    fn frame(&self, progress: &ProgressEvent);
}

/// 服务端：把帧写回发起这条请求的连接。
///
/// 每一帧都带着请求 `id` 回去，客户端才知道这是哪条请求的进度；终帧仍由
/// `serve_ipc` 的 handler 返回，顺序上排在所有帧之后。
#[derive(Debug, Clone)]
pub struct Outlet {
    id: u64,
    tx: mpsc::Sender<Response>,
}

impl Outlet {
    /// `id` 是发起请求的编号（见 [`Request::id`](crate::protocol::Request::id)）。
    pub fn new(id: u64, tx: mpsc::Sender<Response>) -> Self {
        Self { id, tx }
    }

    /// 投递一帧；队列满时返回 `false`。调用方通常不必关心——丢帧是设计的一部分。
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
        // 丢帧是刻意的，也是这里唯一正确的选择：`frame` 会被 `parallel` 分支并发调用，
        // 一旦在这里等待（`send().await`），一条渲染慢的客户端就能把整条流水线拖住。
        let _ = self.offer(progress.clone());
    }
}

/// 客户端：把帧重放进本地的上报口。
///
/// `observer` 为 `None` 时整条路径是空操作，`--quiet` 因此不需要另写一套渲染器。
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
