use std::ffi::OsStr;
use std::fs::File;
use std::io::{self, BufRead, BufReader, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, IntoRawHandle};

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_NONE,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX,
};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, NAMED_PIPE_MODE, PIPE_READMODE_BYTE,
    PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};
use windows::core::PCWSTR;

use crate::serve::ServeOptions;
use crate::serve::dispatch::handle_invoke;
use crate::serve::protocol::{self, Request};
use crate::serve::state::DaemonState;

const PIPE_BUFFER_SIZE: u32 = 65_536;
const MAX_LINE_BYTES: usize = 64 * 1024;

pub fn run_server(options: &ServeOptions, state: &mut DaemonState) -> anyhow::Result<()> {
    let pipe_name = to_wide(&options.pipe_name);

    eprintln!(
        "corex-serve: 监听 Named Pipe {}（Ctrl+C 退出）",
        options.pipe_name
    );

    loop {
        let handle = unsafe {
            CreateNamedPipeW(
                PCWSTR(pipe_name.as_ptr()),
                PIPE_ACCESS_DUPLEX,
                NAMED_PIPE_MODE(PIPE_TYPE_BYTE.0 | PIPE_READMODE_BYTE.0 | PIPE_WAIT.0),
                PIPE_UNLIMITED_INSTANCES,
                PIPE_BUFFER_SIZE,
                PIPE_BUFFER_SIZE,
                0,
                None,
            )
        };

        if handle.is_invalid() {
            anyhow::bail!("CreateNamedPipeW 失败: {}", std::io::Error::last_os_error());
        }

        if let Err(err) = unsafe { ConnectNamedPipe(handle, None) } {
            if err.code().0 as u32 != 535 {
                unsafe {
                    let _ = CloseHandle(handle);
                };
                anyhow::bail!("ConnectNamedPipe 失败: {err}");
            }
        }

        let should_continue = match handle_client(handle, state) {
            Ok(continue_running) => continue_running,
            Err(err) => {
                eprintln!("corex-serve: 客户端处理错误: {err}");
                true
            }
        };

        if !should_continue {
            eprintln!("corex-serve: 收到 shutdown，退出");
            break;
        }
    }

    Ok(())
}

/// 处理单个客户端连接，返回 false 表示 Daemon 应退出
fn handle_client(handle: HANDLE, state: &mut DaemonState) -> anyhow::Result<bool> {
    let file = pipe_file(handle);
    let mut reader = BufReader::new(file);
    let mut result = Ok(true);

    loop {
        match read_line_limited(&mut reader, MAX_LINE_BYTES) {
            Ok(None) => break,
            Ok(Some(line)) => match protocol::parse_request(&line) {
                Ok(Request::Shutdown) => {
                    result = Ok(false);
                    break;
                }
                Ok(Request::Invoke {
                    id,
                    module,
                    action,
                    format,
                    algorithm,
                    args,
                }) => {
                    let response =
                        handle_invoke(state, id, &module, action, format, algorithm, args);
                    write_response(reader.get_mut(), &response)?;
                }
                Err(err) => {
                    let response = protocol::Response::failure(0, err.to_string(), 0);
                    write_response(reader.get_mut(), &response)?;
                }
            },
            Err(err) => {
                result = Err(err.into());
                break;
            }
        }
    }

    disconnect_pipe_file(reader.into_inner());
    result
}

fn write_response(file: &mut File, response: &protocol::Response) -> anyhow::Result<()> {
    let json = serde_json::to_string(response)?;
    file.write_all(json.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}

fn read_line_limited(reader: &mut impl BufRead, max_bytes: usize) -> io::Result<Option<String>> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];

    loop {
        match reader.read(&mut byte)? {
            0 => {
                return if buf.is_empty() {
                    Ok(None)
                } else {
                    Ok(Some(String::from_utf8_lossy(&buf).into_owned()))
                };
            }
            _ => {
                if buf.len() >= max_bytes {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("请求行超过 {max_bytes} 字节限制"),
                    ));
                }
                if byte[0] == b'\n' {
                    return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
                }
                buf.push(byte[0]);
            }
        }
    }
}

fn pipe_file(handle: HANDLE) -> File {
    unsafe { File::from_raw_handle(handle.0 as _) }
}

fn disconnect_pipe_file(file: File) {
    let raw = file.into_raw_handle();
    let handle = HANDLE(raw as _);
    unsafe {
        let _ = DisconnectNamedPipe(handle);
        let _ = CloseHandle(handle);
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn open_pipe_file(pipe_name: &str) -> anyhow::Result<File> {
    let pipe_name_wide = to_wide(pipe_name);
    let handle = unsafe {
        CreateFileW(
            PCWSTR(pipe_name_wide.as_ptr()),
            FILE_GENERIC_READ.0 | FILE_GENERIC_WRITE.0,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_ATTRIBUTE_NORMAL,
            None,
        )
    }
    .map_err(|err| anyhow::anyhow!("无法连接 Named Pipe {pipe_name}: {err}"))?;

    Ok(pipe_file(handle))
}

/// IPC 客户端：发送请求并读取响应
pub fn send_request(
    pipe_name: &str,
    module: &str,
    wire: crate::invoke::WireArgs,
    id: u64,
) -> anyhow::Result<protocol::Response> {
    let mut file = open_pipe_file(pipe_name)?;

    let mut request = serde_json::json!({
        "type": "invoke",
        "id": id,
        "module": module,
        "args": wire.flags,
    });
    if let Some(action) = wire.action {
        request["action"] = serde_json::Value::String(action);
    }
    if let Some(format) = wire.format {
        request["format"] = serde_json::Value::String(format);
    }
    if let Some(algorithm) = wire.algorithm {
        request["algorithm"] = serde_json::Value::String(algorithm);
    }
    let payload = serde_json::to_string(&request)?;
    file.write_all(payload.as_bytes())?;
    file.write_all(b"\n")?;
    file.flush()?;

    let response_line = {
        let mut reader = BufReader::new(&file);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        line
    };

    drop(file);

    let response: protocol::Response = serde_json::from_str(response_line.trim())?;
    Ok(response)
}

pub fn send_shutdown(pipe_name: &str) -> anyhow::Result<()> {
    let mut file = match open_pipe_file(pipe_name) {
        Ok(file) => file,
        Err(_) => return Ok(()),
    };

    file.write_all(br#"{"type":"shutdown"}"#)?;
    file.write_all(b"\n")?;
    file.flush()?;
    Ok(())
}
