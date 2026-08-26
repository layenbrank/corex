//! IPC protocol and transports for talking to `corex-daemon`.

pub mod protocol;
pub mod transport;

pub use protocol::{Request, Response, RpcError, MAX_LINE_BYTES};
pub use transport::{
    default_endpoint, platform_data_dir, platform_transport, serve_platform, PlatformTransport,
    Transport, TransportError,
};

#[cfg(unix)]
pub use transport::UnixSocketTransport;
#[cfg(windows)]
pub use transport::NamedPipeTransport;
