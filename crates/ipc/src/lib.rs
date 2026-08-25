//! IPC protocol and transports for talking to `corex-daemon`.

pub mod protocol;
pub mod transport;

pub use protocol::{Request, Response, RpcError};
pub use transport::{Transport, UnixSocketTransport};
