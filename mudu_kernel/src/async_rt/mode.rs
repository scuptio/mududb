#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncMode {
    Tokio,
    IoUring,
}
