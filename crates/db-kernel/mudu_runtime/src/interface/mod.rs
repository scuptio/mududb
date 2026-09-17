//! Host-side interface functions exposed to guest modules.

pub mod kernel_async;
pub mod kernel_sync;
#[cfg(test)]
mod kernel_test;
