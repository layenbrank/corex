//! 守护触发执行的进度落盘：宿主 / daemon 订阅侧读这份 NDJSON。
//!
//! 每行一个 JSON 对象：
//! - `{"phase":"start","run_id","kind","name","at_ms"}`
//! - `{"phase":"progress","run_id","progress":{…与 ProgressEvent 同形…}}`
//! - `{"phase":"end","run_id","ok","error?","at_ms"}`
//!
//! 文件：`<data>/<kind>/<id>/progress.ndjson`（每次触发截断重写）。
//! 进行中另写 `run.json`，结束时删掉——订阅方靠它发现「有没有在跑」。

use corex_core::{Mark, Observer, Spot, Stream, Unit};
use serde::Serialize;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::warn;

use crate::supervisor::job::JobKind;

/// 进行中标记：存在即「这条守护正在跑一次指令」。
pub fn run_marker_path(job_dir: &Path) -> PathBuf {
    job_dir.join("run.json")
}

/// 进度 NDJSON。
pub fn progress_log_path(job_dir: &Path) -> PathBuf {
    job_dir.join("progress.ndjson")
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// 把 Observer 回调写成 NDJSON，供 daemon / 宿主订阅。
#[derive(Debug)]
pub struct FileProgress {
    kind: JobKind,
    name: String,
    run_id: String,
    job_dir: PathBuf,
    seq: AtomicU64,
    file: Mutex<File>,
}

impl FileProgress {
    /// 开一次新的触发运行：截断进度文件、写下 start 行与 `run.json`。
    pub fn begin_run(job_dir: &Path, kind: JobKind, name: &str) -> std::io::Result<Self> {
        std::fs::create_dir_all(job_dir)?;
        let run_id = format!("{}-{}", name, now_ms());
        let path = progress_log_path(job_dir);
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;
        let start = Line::Start {
            phase: "start",
            run_id: run_id.clone(),
            kind: kind.as_str(),
            name: name.to_string(),
            at_ms: now_ms(),
        };
        writeln!(file, "{}", serde_json::to_string(&start)?)?;
        file.flush()?;

        let marker = RunMarker {
            run_id: run_id.clone(),
            kind: kind.as_str(),
            name: name.to_string(),
            at_ms: now_ms(),
        };
        std::fs::write(run_marker_path(job_dir), serde_json::to_string_pretty(&marker)?)?;

        Ok(Self {
            kind,
            name: name.to_string(),
            run_id,
            job_dir: job_dir.to_path_buf(),
            seq: AtomicU64::new(0),
            file: Mutex::new(file),
        })
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    fn next_seq(&self) -> u64 {
        self.seq.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn write_line(&self, line: &impl Serialize) {
        let Ok(raw) = serde_json::to_string(line) else {
            return;
        };
        let Ok(mut file) = self.file.lock() else {
            return;
        };
        if writeln!(file, "{raw}").is_err() || file.flush().is_err() {
            warn!(
                job = %self.name,
                kind = self.kind.as_str(),
                "写进度帧失败"
            );
        }
    }

    /// 跑完：写 end 行并去掉 `run.json`。
    pub fn finish(&self, ok: bool, error: Option<String>) {
        let end = Line::End {
            phase: "end",
            run_id: self.run_id.clone(),
            ok,
            error,
            at_ms: now_ms(),
        };
        self.write_line(&end);
        let _ = std::fs::remove_file(run_marker_path(&self.job_dir));
    }
}

impl Observer for FileProgress {
    fn begin(&self, at: Spot<'_>) {
        self.write_line(&Line::Progress {
            phase: "progress",
            run_id: self.run_id.clone(),
            progress: ProgressWire::StepStart {
                kind: "step_start",
                seq: self.next_seq(),
                step: at.id.to_string(),
                action: at.action.to_string(),
            },
        });
    }

    fn chunk(&self, at: Spot<'_>, mark: Mark) {
        self.write_line(&Line::Progress {
            phase: "progress",
            run_id: self.run_id.clone(),
            progress: ProgressWire::StepProgress {
                kind: "step_progress",
                step: at.id.to_string(),
                action: at.action.to_string(),
                done: mark.done,
                total: mark.total,
                unit: mark.unit,
            },
        });
    }

    fn output(&self, at: Spot<'_>, stream: Stream, text: &str) {
        self.write_line(&Line::Progress {
            phase: "progress",
            run_id: self.run_id.clone(),
            progress: ProgressWire::StepOutput {
                kind: "step_output",
                step: at.id.to_string(),
                action: at.action.to_string(),
                stream,
                text: text.to_string(),
            },
        });
    }

    fn end(&self, at: Spot<'_>, took: Duration, ok: bool) {
        self.write_line(&Line::Progress {
            phase: "progress",
            run_id: self.run_id.clone(),
            progress: ProgressWire::StepEnd {
                kind: "step_end",
                step: at.id.to_string(),
                action: at.action.to_string(),
                took_ms: took.as_millis() as u64,
                ok,
            },
        });
    }
}

#[derive(Serialize)]
struct RunMarker {
    run_id: String,
    kind: &'static str,
    name: String,
    at_ms: u64,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Line {
    Start {
        phase: &'static str,
        run_id: String,
        kind: &'static str,
        name: String,
        at_ms: u64,
    },
    Progress {
        phase: &'static str,
        run_id: String,
        progress: ProgressWire,
    },
    End {
        phase: &'static str,
        run_id: String,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
        at_ms: u64,
    },
}

/// 与 `corex_ipc::ProgressEvent` 同形的落盘形状（engine 不依赖 ipc）。
#[derive(Serialize)]
#[serde(untagged)]
enum ProgressWire {
    StepStart {
        kind: &'static str,
        seq: u64,
        step: String,
        action: String,
    },
    StepProgress {
        kind: &'static str,
        step: String,
        action: String,
        done: u64,
        total: Option<u64>,
        unit: Unit,
    },
    StepOutput {
        kind: &'static str,
        step: String,
        action: String,
        stream: Stream,
        text: String,
    },
    StepEnd {
        kind: &'static str,
        step: String,
        action: String,
        took_ms: u64,
        ok: bool,
    },
}
