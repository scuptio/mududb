/// Identifies a concrete system provider implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderType {
    /// Tokio-based provider.
    Tokio,
    /// io_uring-based provider.
    IoUring,
}
