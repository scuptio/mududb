//! Public task runtime, spawning, and blocking task helpers.
#![allow(missing_docs)]
#[cfg(not(target_arch = "wasm32"))]
pub use crate::imp::task::async_;
pub use crate::imp::task::context;
pub use crate::imp::task::id;
pub use crate::imp::task::sync;
pub use crate::imp::task::trace;
