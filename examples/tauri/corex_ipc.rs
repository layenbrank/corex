//! Tauri-side Corex IPC client (v4)
//!
//! Copy into a Tauri project as `src-tauri/src/corex_ipc.rs`.
//!
//! ## Dependencies (`src-tauri/Cargo.toml`)
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
//! ## Register (`src-tauri/src/lib.rs`)
//!
//! ```rust
//! mod corex_ipc;
//!
//! // On startup: spawn corex-daemon (sidecar).
//! let _ = corex_ipc::spawn_daemon(corex_ipc::daemon_exe_path());
//!
//! // On exit: corex_ipc::shutdown()?;
//!
//! #[tauri::command]
//! fn take_screenshot(to: String) -> Result<String, String> {
//!     corex_ipc::screenshot(&to)
//! }
//! ```
//!
//! Protocol: NDJSON `Request` / `Response` with `auth_token`.
//! See `docs/ipc-protocol.md`.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(windows)]
use std::ffi::OsStr;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Default IPC endpoint.
/// - Windows: named pipe `\\.\pipe\corex`
/// - Unix: override via `COREX_SOCKET` or pass an absolute path to `spawn_daemon` / `exchange`
#[cfg(windows)]
pub const PIPE_NAME: &str = r"\\.\pipe\corex";
#[cfg(not(windows))]
pub const PIPE_NAME: &str = "corex.sock";

static REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Daemon → client response (`crates/ipc` `Response`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Pong {
        id: u64,
    },
    Ok {
        id: u64,
        #[serde(default)]
        data: Value,
    },
    Error {
        id: u64,
        error: RpcError,
    },
    Bye {
        id: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

/// Resolve auth token: `COREX_TOKEN`, else `<data_dir>/token` when discoverable.
pub fn auth_token() -> Result<String, String> {
    if let Ok(t) = std::env::var("COREX_TOKEN") {
        if !t.is_empty() {
            return Ok(t);
        }
    }
    let candidates = [
        std::env::var("COREX_DATA_DIR").ok().map(|d| PathBuf::from(d).join("token")),
        dirs_hint_token(),
    ];
    for path in candidates.into_iter().flatten() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Ok(trimmed.to_string());
            }
        }
    }
    Err(
        "missing auth token: set COREX_TOKEN or ensure <data-dir>/token exists (same as corex-daemon)"
            .into(),
    )
}

fn dirs_hint_token() -> Option<PathBuf> {
    // Best-effort: common Linux path; Windows/macOS hosts should set COREX_TOKEN.
    let home = std::env::var_os("HOME")?;
    Some(PathBuf::from(home).join(".local/share/corex/token"))
}

/// Sidecar / sibling `corex-daemon` path (adjust for packing).
pub fn daemon_exe_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.parent().map(|d| {
                #[cfg(windows)]
                {
                    d.join("corex-daemon.exe")
                }
                #[cfg(not(windows))]
                {
                    d.join("corex-daemon")
                }
            })
        })
        .unwrap_or_else(|| {
            #[cfg(windows)]
            {
                PathBuf::from("corex-daemon.exe")
            }
            #[cfg(not(windows))]
            {
                PathBuf::from("corex-daemon")
            }
        })
}

/// Spawn `corex-daemon` once at app start.
pub fn spawn_daemon(exe: impl AsRef<Path>) -> Result<Child, String> {
    let endpoint = endpoint_path();
    Command::new(exe.as_ref())
        .arg("--socket")
        .arg(&endpoint)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("failed to start corex-daemon: {e}"))
}

fn endpoint_path() -> String {
    if let Ok(p) = std::env::var("COREX_SOCKET") {
        if !p.is_empty() {
            return p;
        }
    }
    PIPE_NAME.to_string()
}

/// Invoke a single Action by id (v4).
pub fn invoke_action(action: &str, params: Value) -> Result<Response, String> {
    let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let token = auth_token()?;
    let payload = json!({
        "type": "invoke",
        "id": id,
        "auth_token": token,
        "action": action,
        "params": params,
    });
    exchange(&payload.to_string())
}

/// Screenshot helper → `capture.screenshot`.
pub fn screenshot(to: impl AsRef<str>) -> Result<String, String> {
    let resp = invoke_action(
        "capture.screenshot",
        json!({ "to": to.as_ref() }),
    )?;
    match resp {
        Response::Ok { data, .. } => value_to_path(&data)
            .ok_or_else(|| "screenshot ok but no path in data".to_string()),
        Response::Error { error, .. } => Err(format!("[{}] {}", error.code, error.message)),
        other => Err(format!("unexpected response: {other:?}")),
    }
}

fn value_to_path(data: &Value) -> Option<String> {
    if let Some(s) = data.as_str() {
        return Some(s.to_string());
    }
    if let Some(obj) = data.as_object() {
        if let Some(p) = obj.get("path").and_then(|v| v.as_str()) {
            return Some(p.to_string());
        }
        // Corex Value::File often serializes as a plain string; also accept nested.
        if let Some(p) = obj.get("File").and_then(|v| v.as_str()) {
            return Some(p.to_string());
        }
    }
    None
}

/// Probe whether the endpoint accepts a connection.
pub fn is_ready() -> bool {
    open_endpoint(&endpoint_path()).is_ok()
}

/// Ask the daemon to exit.
pub fn shutdown() -> Result<(), String> {
    let id = REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    let token = auth_token()?;
    let payload = json!({
        "type": "shutdown",
        "id": id,
        "auth_token": token,
    });
    let _ = exchange(&payload.to_string())?;
    Ok(())
}

fn exchange(request_json: &str) -> Result<Response, String> {
    let mut file = open_endpoint(&endpoint_path())?;
    file.write_all(request_json.as_bytes())
        .map_err(|e| e.to_string())?;
    file.write_all(b"\n").map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;

    let mut reader = BufReader::new(&file);
    let mut line = String::new();
    reader.read_line(&mut line).map_err(|e| e.to_string())?;
    serde_json::from_str(line.trim()).map_err(|e| format!("failed to parse response: {e}"))
}

fn open_endpoint(endpoint: &str) -> Result<File, String> {
    #[cfg(windows)]
    {
        open_pipe(endpoint)
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::net::UnixStream;
        let stream = UnixStream::connect(endpoint)
            .map_err(|e| format!("cannot connect to {endpoint}: {e}"))?;
        Ok(unsafe {
            use std::os::unix::io::{FromRawFd, IntoRawFd};
            File::from_raw_fd(stream.into_raw_fd())
        })
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
    .map_err(|e| format!("cannot connect to {pipe_name}: {e}"))?;

    Ok(unsafe { File::from_raw_handle(handle.0 as _) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn response_ok_roundtrip() {
        let raw = r#"{"type":"ok","id":1,"data":{"path":"C:/a.png"}}"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        match resp {
            Response::Ok { id, data } => {
                assert_eq!(id, 1);
                assert_eq!(data.get("path").and_then(|v| v.as_str()), Some("C:/a.png"));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn response_error_roundtrip() {
        let raw = r#"{"type":"error","id":2,"error":{"code":401,"message":"unauthorized"}}"#;
        let resp: Response = serde_json::from_str(raw).unwrap();
        match resp {
            Response::Error { id, error } => {
                assert_eq!(id, 2);
                assert_eq!(error.code, 401);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
