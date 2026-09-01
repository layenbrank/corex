//! IPC protocol and transports for talking to `corex-daemon`.

pub mod protocol;
pub mod transport;

pub use protocol::{MAX_LINE_BYTES, Request, Response, RpcError};
pub use transport::{
    PlatformTransport, Transport, TransportError, config_paths, data_dir, ipc_connect,
    ipc_endpoint, serve_ipc,
};

#[cfg(windows)]
pub use transport::NamedPipeTransport;
#[cfg(unix)]
pub use transport::UnixSocketTransport;
