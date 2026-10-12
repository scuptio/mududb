use mudu_sys::common::provider_type::ProviderType;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::provider::create_io_provider;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Backend server execution mode.
#[derive(Eq, PartialEq, Debug, Clone, Copy, Default)]
pub enum ServerMode {
    /// Legacy mode.
    Legacy,
    /// io_uring-based mode.
    IOUring,
    /// Tokio-based mode.
    #[default]
    Tokio,
}

impl Serialize for ServerMode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{:?}", self))
    }
}

impl<'de> Deserialize<'de> for ServerMode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.to_ascii_lowercase().as_str() {
            "legacy" => Ok(ServerMode::Legacy),
            "iouring" | "io_uring" => Ok(ServerMode::IOUring),
            "tokio" => Ok(ServerMode::Tokio),
            other => Err(serde::de::Error::custom(format!(
                "unknown server mode: {}",
                other
            ))),
        }
    }
}

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
    /// Server-owned budgets applied to every procedure invocation.
    pub invocation_limits: super::invocation_limits::InvocationLimits,
    /// Target component model version.
    pub component_target: ComponentTarget,
    /// Whether async runtime support is enabled.
    pub enable_async: bool,
    /// Selected server mode.
    pub sever_mode: ServerMode,
    /// Optional async I/O provider.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Defer initdb DDL execution during startup auto-load.
    ///
    /// Kernel server modes (Tokio / io_uring) set this so package auto-load
    /// records pending initdb work instead of connecting to the server's own
    /// not-yet-bound TCP listener; the recorded DDL is applied later via
    /// `drain_initdb`. The install/reload paths keep this `false` and execute
    /// initdb immediately.
    pub defer_initdb: bool,
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
            invocation_limits: Default::default(),
            component_target: ComponentTarget::P2,
            enable_async: false,
            sever_mode: Default::default(),
            async_runtime: None,
            defer_initdb: false,
        }
    }
}

impl std::fmt::Debug for RuntimeOpt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeOpt")
            .field("invocation_limits", &self.invocation_limits)
            .field("component_target", &self.component_target)
            .field("enable_async", &self.enable_async)
            .field("sever_mode", &self.sever_mode)
            .field("has_async_runtime", &self.async_runtime.is_some())
            .field("defer_initdb", &self.defer_initdb)
            .finish()
    }
}
