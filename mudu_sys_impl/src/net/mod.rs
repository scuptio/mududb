pub mod async_;
pub mod sync;

// Re-export async types at net root for backward compatibility
pub use async_::{AsyncTcpListener, AsyncTcpStream};
