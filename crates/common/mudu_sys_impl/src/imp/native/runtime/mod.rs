#![allow(missing_docs)]
use crate::common::provider_type::ProviderType;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::contract::async_mode::AsyncMode;
use crate::contract::io_provider_base::IoProviderBase;
#[cfg(target_os = "linux")]
use crate::imp::fs::AsyncIoUringFs;
use crate::imp::fs::AsyncTokioFs;
#[cfg(target_os = "linux")]
use crate::imp::native::net::async_::AsyncIoUringNet;
use crate::imp::native::net::async_::TokioNet;
use std::sync::Arc;

pub enum Runtime {
    Tokio(IoProviderBase<TokioNet, AsyncTokioFs>),
    #[cfg(target_os = "linux")]
    IoUring(IoProviderBase<AsyncIoUringNet, AsyncIoUringFs>),
}

pub fn create_async_runtime(mode: ProviderType) -> Arc<dyn AsyncIoProvider> {
    Runtime::from(mode)
}

impl Runtime {
    pub(crate) fn from(mode: ProviderType) -> Arc<dyn AsyncIoProvider> {
        match mode {
            #[cfg(target_os = "linux")]
            ProviderType::IoUring => Arc::new(IoProviderBase::new_with(
                AsyncMode::IoUring,
                Arc::new(AsyncIoUringNet::new()),
                Arc::new(AsyncIoUringFs::new()),
            )),
            ProviderType::Tokio => Arc::new(IoProviderBase::new_with(
                AsyncMode::Tokio,
                Arc::new(TokioNet::new()),
                Arc::new(AsyncTokioFs::new()),
            )),
        }
    }

    pub fn new() -> Self {
        #[cfg(target_os = "linux")]
        {
            if crate::io_uring_available() {
                return Self::IoUring(IoProviderBase::new_with(
                    AsyncMode::IoUring,
                    Arc::new(AsyncIoUringNet::new()),
                    Arc::new(AsyncIoUringFs::new()),
                ));
            }
        }
        Self::Tokio(IoProviderBase::new_with(
            AsyncMode::Tokio,
            Arc::new(TokioNet::new()),
            Arc::new(AsyncTokioFs::new()),
        ))
    }

    pub fn mode(&self) -> AsyncMode {
        match self {
            Runtime::Tokio(_) => AsyncMode::Tokio,
            #[cfg(target_os = "linux")]
            Runtime::IoUring(_) => AsyncMode::IoUring,
        }
    }
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[cfg_attr(miri, ignore)]
    #[test]
    fn create_async_runtime_tokio_has_tokio_mode() {
        let provider = create_async_runtime(ProviderType::Tokio);
        assert_eq!(provider.mode(), AsyncMode::Tokio);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn runtime_from_tokio_has_tokio_mode() {
        let provider = Runtime::from(ProviderType::Tokio);
        assert_eq!(provider.mode(), AsyncMode::Tokio);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn runtime_new_mode_matches_io_uring_availability() {
        let runtime = Runtime::new();
        #[cfg(target_os = "linux")]
        {
            let expected = if crate::io_uring_available() {
                AsyncMode::IoUring
            } else {
                AsyncMode::Tokio
            };
            assert_eq!(runtime.mode(), expected);
        }
        #[cfg(not(target_os = "linux"))]
        {
            assert_eq!(runtime.mode(), AsyncMode::Tokio);
        }
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn runtime_default_equals_new() {
        let default = Runtime::default();
        let new = Runtime::new();
        assert_eq!(default.mode(), new.mode());
    }

    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)]
    #[test]
    fn create_async_runtime_io_uring_has_io_uring_mode() {
        let provider = create_async_runtime(ProviderType::IoUring);
        assert_eq!(provider.mode(), AsyncMode::IoUring);
    }

    #[cfg_attr(miri, ignore)]
    #[test]
    fn tokio_provider_trait_objects_are_usable() {
        let provider = create_async_runtime(ProviderType::Tokio);
        let _ = provider.net();
        let _ = provider.fs();
        let _ = provider.fs_arc();
    }
}
