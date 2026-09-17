#![allow(missing_docs)]
use crate::common::provider_type::ProviderType;
use crate::contract::async_fs::AsyncFs;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::imp::fs::sync::FsSync;
use crate::provider::create_io_provider;
use std::sync::{Arc, OnceLock};

#[derive(Clone)]
pub struct SysIoContext {
    io: Arc<dyn AsyncIoProvider>,
}

impl SysIoContext {
    pub fn new(io: Arc<dyn AsyncIoProvider>) -> Arc<Self> {
        Arc::new(Self { io })
    }

    pub fn with_provider(provider_type: ProviderType) -> Arc<Self> {
        Self::new(create_io_provider(provider_type))
    }

    pub fn tokio() -> Arc<Self> {
        Self::with_provider(ProviderType::Tokio)
    }

    #[cfg(target_os = "linux")]
    pub fn iouring() -> Arc<Self> {
        Self::with_provider(ProviderType::IoUring)
    }

    pub fn provider(&self) -> &dyn AsyncIoProvider {
        self.io.as_ref()
    }

    pub fn provider_arc(&self) -> Arc<dyn AsyncIoProvider> {
        self.io.clone()
    }

    pub fn fs(&self) -> Arc<dyn AsyncFs> {
        self.io.fs_arc()
    }

    pub fn fs_sync(&self) -> FsSync {
        FsSync::new()
    }
}

pub fn default_sys_io_context() -> Arc<SysIoContext> {
    static DEFAULT: OnceLock<Arc<SysIoContext>> = OnceLock::new();
    DEFAULT.get_or_init(SysIoContext::tokio).clone()
}
