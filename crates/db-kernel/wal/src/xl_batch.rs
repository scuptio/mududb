pub use crate::xl_batch_type::XLBatch;

pub use crate::xl_batch_worker_log::{
    append_xl_batch_async, decode_xl_batches, decode_xl_batches_with_pending, deserialize_batch,
    new_xl_batch_worker_log, new_xl_batch_writer, serialize_batch, NoopXLBatchRecoveryHandler,
    XLBatchWorkerLog,
};

/// Fuzz-target stubs for the transaction log batch codec.
#[cfg(any(test, feature = "test", fuzzing))]
pub mod _fuzz {

    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    /// Fuzz entry point for [`XLBatch`] decode/encode round-trips; the body
    /// lives in the `mudu_kernel` fuzz harness.
    pub fn _de_en_x_l_batch(_data: &[u8]) {}
}
