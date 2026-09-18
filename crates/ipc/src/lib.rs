//! 与 `corex-daemon` 通信的 IPC 协议与传输。

pub mod endpoint;
pub mod progress;
pub mod protocol;
pub mod transport;

pub use progress::{FrameSink, Outlet, ProgressEvent, Replay};
pub use protocol::{MAX_LINE_BYTES, Request, Response, RpcError};
pub use transport::{
    PlatformTransport, Transport, TransportError, config_paths, data_dir, find_endpoint,
    find_token, ipc_connect, ipc_endpoint, resolve_data_relative, resolve_endpoint, serve_ipc,
    serve_ipc_ready,
};

#[cfg(windows)]
pub use transport::NamedPipeTransport;
#[cfg(unix)]
pub use transport::UnixSocketTransport;
