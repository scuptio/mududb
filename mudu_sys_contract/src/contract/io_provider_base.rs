use crate::contract::async_fs::AsyncFs;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::contract::async_mode::AsyncMode;
use crate::contract::async_net::AsyncNet;
use std::sync::Arc;

/// Generic base implementation of an async I/O provider.
pub struct IoProviderBase<N, F> {
    mode: AsyncMode,
    net: Arc<N>,
    fs: Arc<F>,
}

impl<N, F> IoProviderBase<N, F> {
    /// Create a provider from a mode, network implementation and file-system implementation.
    pub fn new_with(mode: AsyncMode, net: Arc<N>, fs: Arc<F>) -> Self {
        Self { mode, net, fs }
    }
}

impl<N: AsyncNet, F: AsyncFs + 'static> AsyncIoProvider for IoProviderBase<N, F> {
    fn mode(&self) -> AsyncMode {
        self.mode
    }

    fn net(&self) -> &dyn AsyncNet {
        self.net.as_ref()
    }

    fn fs(&self) -> &dyn AsyncFs {
        self.fs.as_ref()
    }

    fn fs_arc(&self) -> Arc<dyn AsyncFs> {
        self.fs.clone()
    }
}
