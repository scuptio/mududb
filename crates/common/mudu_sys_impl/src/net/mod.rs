//! Public networking types and async/sync network operations.
#![allow(missing_docs)]
pub mod async_;
pub mod contract;
pub mod sync;
pub mod to_addrs;

// Re-export async types at net root for backward compatibility
pub use async_::{AsyncTcpListener, AsyncTcpStream};
pub use to_addrs::ToAddrs;
