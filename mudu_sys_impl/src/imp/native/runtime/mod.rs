use crate::common::provider_type::ProviderType;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::contract::async_mode::AsyncMode;
use crate::contract::io_provider_base::IoProviderBase;
#[cfg(target_os = "linux")]
use crate::imp::native::fs::async_::AsyncIoUringFs;
use crate::imp::native::fs::async_::AsyncTokioFs;
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
