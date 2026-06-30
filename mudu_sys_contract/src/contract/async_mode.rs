#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Async runtime mode selection.
pub enum AsyncMode {
    /// Use the Tokio runtime.
    Tokio,
    /// Use io_uring.
    IoUring,
}
