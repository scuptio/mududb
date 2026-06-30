use crate::backend::mududb_cfg::ServerMode;
use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::provider::create_io_provider;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Target Wasm component model version.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComponentTarget {
    /// Preview 2 component model.
    #[default]
    P2,
    /// Preview 3 component model.
    P3,
}

/// Options controlling runtime behavior.
#[derive(Clone)]
pub struct RuntimeOpt {
    /// Target component model version.
    pub component_target: ComponentTarget,
    /// Whether async runtime support is enabled.
    pub enable_async: bool,
    /// Selected server mode.
    pub sever_mode: ServerMode,
    /// Optional async I/O provider.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}

impl RuntimeOpt {
    /// Returns the target component model version.
    pub fn component_target(&self) -> ComponentTarget {
        self.component_target
    }

    /// Returns the configured async I/O provider, if any.
    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }

    /// Builds an async I/O provider matching the given server mode, if applicable.
    pub fn build_async_runtime(server_mode: ServerMode) -> Option<Arc<dyn AsyncIoProvider>> {
        let opt_mode = Self::server_mode_to_runtime_mode(server_mode);
        opt_mode.map(create_io_provider)
    }

    fn server_mode_to_runtime_mode(mode: ServerMode) -> Option<ProviderType> {
        match mode {
            ServerMode::Legacy => None,
            ServerMode::IOUring => Some(ProviderType::IoUring),
            ServerMode::Tokio => Some(ProviderType::Tokio),
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
