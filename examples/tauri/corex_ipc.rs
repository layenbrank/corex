//! Tauri 侧 corex IPC 客户端（阶段 4 示例）
//!
//! 复制到 Tauri 项目：`src-tauri/src/corex_ipc.rs`
//!
//! ## 依赖（`src-tauri/Cargo.toml`）
//!
//! ```toml
//! [dependencies]
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//!
//! [target.'cfg(windows)'.dependencies]
//! windows = { version = "0.62", features = [
//!     "Win32_Foundation",
//!     "Win32_Security",
//!     "Win32_Storage_FileSystem",
//! ] }
//! ```
//!
//! ## 注册模块（`src-tauri/src/lib.rs` 或 `main.rs`）
//!
//! ```rust
//! mod corex_ipc;
//!
//! #[cfg_attr(mobile, tauri::mobile_entry_point)]
//! pub fn run() {
//!     // 应用启动时拉起 corex-serve（sidecar 或同目录二进制）
//!     let _ = corex_ipc::spawn_daemon(corex_ipc::daemon_exe_path());
//!
//!     tauri::Builder::default()
//!         .invoke_handler(tauri::generate_handler![take_screenshot])
//!         .build(tauri::generate_context!())
//!         .expect("error while building tauri application")
//!         .run(|_app, event| {
//!             if let tauri::RunEvent::Exit = event {
//!                 let _ = corex_ipc::shutdown();
//!             }
//!         });
//! }
//!
//! #[tauri::command]
//! fn take_screenshot(to: String) -> Result<String, String> {
//!     corex_ipc::screenshot(&to)
//! }
//! ```
//!
//! 配套文件见同目录 `README.md`：
//! - `tauri.conf.json` / `capabilities/default.json` — sidecar 配置
//! - `lib.rs` — 托盘 + 快捷键完整 wiring
//! - `scripts/copy-corex-serve.mjs` — 构建前复制 sidecar 二进制
//!
//! 1. 将 `corex-serve.exe` 放入 Tauri 资源 / sidecar（或 PATH 可找到）
//! 2. 先单独验证：`cargo run -p corex-serve` + `cargo run -p corex-core --example ipc --features serve`
//!
//! ## 协议（与 corex-serve 一致）
//!
//! 请求：`{"type":"invoke","id":1,"module":"screenshot","action":"capture","args":{"to":"C:/out"}}\n`
//! 响应：`{"id":1,"ok":true,"path":"...","ms":87}\n`
//! 关闭：`{"type":"shutdown"}\n`

use std::ffi::OsStr;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// 默认 Named Pipe 名称（与 corex-serve 一致）
pub const PIPE_NAME: &str = r"\\.\pipe\corex";

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// IPC 响应（与 corex-core `serve::protocol::Response` 一致）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    pub ms: u64,
    #[serde(default)]
    pub error: Option<String>,
}

/// 返回 sidecar / 同目录下的 corex-serve 路径，按实际打包方式修改
pub fn daemon_exe_path() -> PathBuf {
    // 示例：与主程序同目录的 corex-serve.exe
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("corex-serve.exe")))
        .unwrap_or_else(|| PathBuf::from("corex-serve.exe"))
}

/// 启动 corex-serve Daemon（应用启动时调用一次）
pub fn spawn_daemon(exe: impl AsRef<Path>) -> Result<Child, String> {
    Command::new(exe.as_ref())
        .arg("--pipe")
        .arg(PIPE_NAME)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("启动 corex-serve 失败: {e}"))
}

/// 调用任意 corex 模块（线格式：可选 action / format / algorithm + 扁平 args）
pub fn invoke(
    module: &str,
    action: Option<&str>,
    format: Option<&str>,
    algorithm: Option<&str>,
    args: Value,
) -> Result<Response, String> {
    let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let mut payload = json!({
        "type": "invoke",
        "id": id,
        "module": module,
        "args": args,
    });
    if let Some(action) = action {
        payload["action"] = json!(action);
    }
    if let Some(format) = format {
        payload["format"] = json!(format);
    }
    if let Some(algorithm) = algorithm {
        payload["algorithm"] = json!(algorithm);
    }
    exchange(&payload.to_string())
}

/// 截图（全局快捷键等高频场景）
pub fn screenshot(to: impl AsRef<str>) -> Result<String, String> {
    let resp = invoke(
        "screenshot",
        Some("capture"),
        None,
        None,
        json!({ "to": to.as_ref() }),
    )?;
    if resp.ok {
        resp.path.ok_or_else(|| "screenshot 成功但未返回 path".to_string())
    } else {
        Err(resp.error.unwrap_or_else(|| "screenshot 失败".to_string()))
    }
}

/// 探测 Named Pipe 是否可连接（不发送业务请求）
pub fn is_ready() -> bool {
    #[cfg(windows)]
    {
        open_pipe(PIPE_NAME).is_ok()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

/// 请求 Daemon 优雅退出（应用关闭时调用）
pub fn shutdown() -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut file = open_pipe(PIPE_NAME)?;
        file.write_all(br#"{"type":"shutdown"}"#)
            .map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("corex IPC 当前仅支持 Windows Named Pipe".to_string())
    }
}

fn exchange(request_json: &str) -> Result<Response, String> {
    #[cfg(windows)]
    {
        let mut file = open_pipe(PIPE_NAME)?;
        file.write_all(request_json.as_bytes())
            .map_err(|e| e.to_string())?;
        file.write_all(b"\n").map_err(|e| e.to_string())?;
        file.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(&file);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;

        serde_json::from_str(line.trim()).map_err(|e| format!("解析响应失败: {e}"))
    }
    #[cfg(not(windows))]
    {
        let _ = request_json;
        Err("corex IPC 当前仅支持 Windows Named Pipe".to_string())
    }
}

#[cfg(windows)]
fn open_pipe(pipe_name: &str) -> Result<File, String> {
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::FromRawHandle;

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE,
        OPEN_EXISTING,
    };

    let wide: Vec<u16> = OsStr::new(pipe_name)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let handle = unsafe {
        CreateFileW(
            PCWSTR(wide.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|e| format!("无法连接 {pipe_name}: {e}"))?;

    Ok(unsafe { File::from_raw_handle(handle.0 as _) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_json_roundtrip() {
        let raw = r#"{"id":1,"ok":true,"path":"C:/a.png","ms":42}"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.path.as_deref(), Some("C:/a.png"));
    }
}
