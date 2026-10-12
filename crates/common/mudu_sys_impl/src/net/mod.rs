//! Public networking types and async/sync network operations.
/// Async TCP listener and stream types.
pub mod async_;
/// Synchronous networking contracts.
pub mod contract;
/// Blocking TCP listener and stream types.
pub mod sync;
/// Address resolution trait abstraction.
pub mod to_addrs;

// Re-export async types at net root for backward compatibility
pub use async_::{AsyncTcpListener, AsyncTcpStream};
pub use to_addrs::ToAddrs;
