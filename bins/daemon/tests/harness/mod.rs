//! 把真 daemon 拉起来的公共脚手架。
//!
//! 守护进程是唯一能加载 WASM 插件的执行者，也是宿主唯一会连的东西，所以它的回话形状
//! 只能靠**起真进程、连真管道**来验。各个用例的脚手架放在这里，别各写一份。

#![allow(dead_code)]

use corex_core::Value;
use corex_ipc::protocol::Request;
use corex_ipc::{Transport, ipc_connect};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// 固定的测试 token：`COREX_TOKEN` 优先于配置文件，两边都给同一个值才不必读文件。
pub const TOKEN: &str = "corex-daemon-test";

/// 起进程的护栏：测试无论怎么退出（含 panic）都要把 daemon 收掉，
/// 否则下一次运行会撞上单实例锁。
pub struct Daemon {
    child: Child,
    log: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Daemon {
    fn spawn(config: &Path, directives: &Path, log: &Path, data: &Path) -> Self {
        let out = std::fs::File::create(log).expect("daemon log");
        let err = out.try_clone().expect("daemon log clone");
        let child = Command::new(env!("CARGO_BIN_EXE_corex-daemon"))
            .arg("--config")
            .arg(config)
            .arg("--directives")
            .arg(directives)
            .env("COREX_TOKEN", TOKEN)
            // 钉住数据目录。不钉的话 `data_dir()` 会退到二进制所在的目录（构建产物旁边），
            // 端点记录与历史都写到那儿去，而且几个用例共用一份会互相覆盖。
            .env("COREX_DATA_DIR", data)
            .stdin(Stdio::null())
            .stdout(Stdio::from(out))
            .stderr(Stdio::from(err))
            .spawn()
            .expect("spawn corex-daemon");
        Self {
            child,
            log: log.to_path_buf(),
        }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// 等到进程自己退出。只有走有序清理路径（收 `shutdown`）才会发生。
    pub async fn wait_exit(&mut self) {
        for _ in 0..300 {
            if let Some(status) = self.child.try_wait().expect("poll daemon") {
                assert!(
                    status.success(),
                    "corex-daemon 退出码异常 {status}:\n{}",
                    self.log_text()
                );
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("corex-daemon 没有在 6s 内退出:\n{}", self.log_text());
    }

    pub fn log_text(&self) -> String {
        std::fs::read_to_string(&self.log).unwrap_or_default()
    }

    /// 连上为止；连不上就把 daemon 的日志贴出来——否则失败只剩一句“连接失败”。
    async fn wait_ready(&mut self, endpoint: &Path, token: &str) {
        for _ in 0..300 {
            if let Some(status) = self.child.try_wait().expect("poll daemon") {
                panic!(
                    "corex-daemon 提前退出（{status}）:\n{}",
                    std::fs::read_to_string(&self.log).unwrap_or_default()
                );
            }
            let ping = Request::Ping {
                id: 0,
                auth_token: None,
            }
            .with_auth_token(token);
            if ipc_connect(endpoint).send(&ping).await.is_ok() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!(
            "corex-daemon 没有在 6s 内就绪:\n{}",
            std::fs::read_to_string(&self.log).unwrap_or_default()
        );
    }
}

/// 每个用例自己的端点与锁：命名管道是全局的，而测试是并行跑的。
pub fn endpoint(tag: &str) -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(format!(
            r"\\.\pipe\corex-daemon-test-{}-{tag}",
            std::process::id()
        ))
    }
    #[cfg(unix)]
    {
        std::env::temp_dir().join(format!(
            "corex-daemon-test-{}-{tag}.sock",
            std::process::id()
        ))
    }
}

/// 写一份只服务这个用例的配置：端点、锁、token 都钉住，不碰用户的数据目录。
///
/// `extra` 原样接在 `[daemon]` 里，用来直接写要测的那个键（如 `max_jobs = 2`）。
fn write_config(dir: &Path, endpoint: &Path, extra: &str) -> PathBuf {
    let path = dir.join("corex-daemon-test.toml");
    // 用 TOML 的字面量字符串（单引号）：Windows 管道路径里的反斜杠不该被当成转义。
    let text = format!(
        "[daemon]\nsocket_path = '{}'\nlock_path = '{}'\ntoken = '{TOKEN}'\n{extra}",
        endpoint.display(),
        dir.join("daemon.lock").display()
    );
    std::fs::write(&path, text).expect("write config");
    path
}

/// 起一个 daemon，并把它的指令目录与端点一起交回来。
pub async fn start(tag: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    start_with(tag, "").await
}

/// 同上，但可以往 `[daemon]` 里多写几行。
pub async fn start_with(tag: &str, extra: &str) -> (tempfile::TempDir, Daemon, PathBuf) {
    let dir = tempfile::tempdir().expect("temp dir");
    let directives = dir.path().join("directives");
    std::fs::create_dir_all(&directives).expect("directives dir");
    let endpoint = endpoint(tag);
    let config = write_config(dir.path(), &endpoint, extra);
    let mut daemon = Daemon::spawn(
        &config,
        &directives,
        &dir.path().join("daemon.log"),
        dir.path(),
    );
    daemon.wait_ready(&endpoint, TOKEN).await;
    (dir, daemon, endpoint)
}

pub fn authed(request: Request) -> Request {
    request.with_auth_token(TOKEN)
}

/// 取一个字符串数组字段；缺失或类型不对都当空表。
pub fn strings(value: &Value, path: &str) -> Vec<String> {
    value
        .find_path(path)
        .and_then(|v| v.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}
