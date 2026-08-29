//! IPC protocol and transports for talking to `corex-daemon`.

pub mod protocol;
pub mod transport;

pub use protocol::{Request, Response, RpcError, MAX_LINE_BYTES};
pub use transport::{
    config_paths, data_dir, ipc_connect, ipc_endpoint, serve_ipc, PlatformTransport, Transport,
    TransportError,
};

#[cfg(unix)]
pub use transport::UnixSocketTransport;
#[cfg(windows)]
pub use transport::NamedPipeTransport;
