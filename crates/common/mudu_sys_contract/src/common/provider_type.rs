/// Identifies a concrete system provider implementation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderType {
    /// Tokio-based provider.
    Tokio,
    /// Linux io_uring-based provider.
    #[cfg(target_os = "linux")]
    IoUring,
}
