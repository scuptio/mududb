use crate::async_rt::contract::{AsyncFs, AsyncNet, AsyncRuntime};
use crate::async_rt::linux::fs::IoUringFs;
use crate::async_rt::linux::net::IoUringNet;
use crate::async_rt::mode::AsyncMode;
use std::sync::Arc;

pub struct IoUringRuntime {
    net: Arc<IoUringNet>,
    fs: Arc<IoUringFs>,
}

impl IoUringRuntime {
    pub fn new() -> Self {
        Self {
            net: Arc::new(IoUringNet::new()),
            fs: Arc::new(IoUringFs::new()),
        }
    }
}

impl Default for IoUringRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncRuntime for IoUringRuntime {
    fn mode(&self) -> AsyncMode {
        AsyncMode::IoUring
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
