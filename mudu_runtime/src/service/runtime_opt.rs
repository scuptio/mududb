use crate::backend::mududb_cfg::ServerMode;
use mudu_kernel::async_rt::contract::AsyncRuntime;
#[cfg(target_os = "linux")]
use mudu_kernel::async_rt::linux::runtime::IoUringRuntime;
use mudu_kernel::async_rt::tokio::runtime::TokioRuntime;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComponentTarget {
    #[default]
    P2,
    P3,
}

#[derive(Clone)]
pub struct RuntimeOpt {
    pub component_target: ComponentTarget,
    pub enable_async: bool,
    pub sever_mode: ServerMode,
    pub async_runtime: Option<Arc<dyn AsyncRuntime>>,
}

impl RuntimeOpt {
    pub fn component_target(&self) -> ComponentTarget {
        self.component_target
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncRuntime>> {
        self.async_runtime.clone()
    }

    pub fn build_async_runtime(server_mode: ServerMode) -> Option<Arc<dyn AsyncRuntime>> {
        match server_mode {
            #[cfg(target_os = "linux")]
            ServerMode::IOUring => Some(Arc::new(IoUringRuntime::new())),
            ServerMode::Tokio => Some(Arc::new(TokioRuntime::new())),
            _ => None,
        }
    }
}

impl Default for RuntimeOpt {
    fn default() -> Self {
        Self {
            component_target: ComponentTarget::P2,
            enable_async: false,
            sever_mode: Default::default(),
            async_runtime: None,
        }
    }
}

impl std::fmt::Debug for RuntimeOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeOpt")
            .field("component_target", &self.component_target)
            .field("enable_async", &self.enable_async)
            .field("sever_mode", &self.sever_mode)
            .field("has_async_runtime", &self.async_runtime.is_some())
            .finish()
    }
}
