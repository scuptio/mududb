use crate::async_rt::contract::{AsyncFs, AsyncNet, AsyncRuntime};
use crate::async_rt::mode::AsyncMode;
use crate::async_rt::tokio::fs::TokioFs;
use crate::async_rt::tokio::net::TokioNet;
use std::sync::Arc;

pub struct TokioRuntime {
    net: Arc<TokioNet>,
    fs: Arc<TokioFs>,
}

impl TokioRuntime {
    pub fn new() -> Self {
        Self {
            net: Arc::new(TokioNet::new()),
            fs: Arc::new(TokioFs::new()),
        }
    }
}

impl Default for TokioRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncRuntime for TokioRuntime {
    fn mode(&self) -> AsyncMode {
        AsyncMode::Tokio
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
