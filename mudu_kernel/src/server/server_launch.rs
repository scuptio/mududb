
use mudu_sys::net::sync::StdTcpListener;

use mudu::common::result::RS;

use crate::server::server_cfg::ServerCfg;
use crate::server::server_runtime_deps::ServerRuntimeDeps;

/// A single server start request, including one-shot resources such as listeners.
pub struct ServerLaunch {
    cfg: ServerCfg,
    deps: ServerRuntimeDeps,
    prebound_listener: Option<StdTcpListener>,
}

impl ServerLaunch {
    pub fn new(cfg: ServerCfg, deps: ServerRuntimeDeps) -> Self {
        Self {
            cfg,
            deps,
            prebound_listener: None,
        }
    }

    pub fn from_cfg(cfg: ServerCfg) -> RS<Self> {
        let deps = ServerRuntimeDeps::from_cfg(&cfg)?;
        Ok(Self::new(cfg, deps))
    }

    pub fn with_prebound_listener(mut self, listener: StdTcpListener) -> Self {
        self.prebound_listener = Some(listener);
        self
    }

    pub fn cfg(&self) -> &ServerCfg {
        &self.cfg
    }

    pub fn deps(&self) -> &ServerRuntimeDeps {
        &self.deps
    }

    pub fn take_prebound_listener(&mut self) -> Option<StdTcpListener> {
        self.prebound_listener.take()
    }
}

/// Alias used by backend construction code that does not need a transport-specific name.
pub type WorkerTcpBackendConfig = ServerLaunch;
