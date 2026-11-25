//! Asynchronous persistence boundary of the protocol driver.

use std::future::Future;
use std::pin::Pin;

use mudu::common::result::RS;

use super::RaftStore;
use crate::core::message::WriteOp;

/// An owned-store IO operation that must be driven through completion.
pub type StoreFuture<'a> = Pin<Box<dyn Future<Output = RS<()>> + Send + 'a>>;

/// Single-writer persistence used by a serialized Raft driver.
/// Implementations finish all writes and required fsyncs before acknowledging a
/// batch. An IO error makes the store unusable until recovery; callers must not
/// cancel an operation and then reuse its files or buffers.
pub trait AsyncStore: Send {
    /// Persists the whole ordered batch before returning success.
    fn apply_batch<'a>(&'a mut self, ops: &'a [WriteOp]) -> StoreFuture<'a>;
    /// Drains outstanding writes and makes the final store frontier durable.
    fn flush(&mut self) -> StoreFuture<'_>;
}

impl AsyncStore for RaftStore {
    fn apply_batch<'a>(&'a mut self, ops: &'a [WriteOp]) -> StoreFuture<'a> {
        Box::pin(self.apply_batch_async(ops))
    }

    fn flush(&mut self) -> StoreFuture<'_> {
        Box::pin(self.flush_async())
    }
}
