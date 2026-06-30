use mudu_sys::net::sync::StdTcpListener;

use mudu::common::result::RS;

use crate::server::server_cfg::ServerCfg;
use crate::server::server_runtime_deps::ServerRuntimeDeps;

/// A single server start request, including one-shot resources such as listeners.
pub struct ServerLaunch {
    cfg: ServerCfg,
    deps: ServerRuntimeDeps,
    prebound_listeners: Vec<Option<StdTcpListener>>,
}

impl ServerLaunch {
    pub fn new(cfg: ServerCfg, deps: ServerRuntimeDeps) -> Self {
        Self {
            cfg,
            deps,
            prebound_listeners: Vec::new(),
        }
    }

    pub fn from_cfg(cfg: ServerCfg) -> RS<Self> {
        let deps = ServerRuntimeDeps::from_cfg(&cfg)?;
        Ok(Self::new(cfg, deps))
    }

    /// Supplies a single pre-bound listener for worker 0.
    ///
    /// For multi-port configurations use [`Self::with_prebound_listeners`] so
    /// that every worker receives its own reserved listener.
    pub fn with_prebound_listener(mut self, listener: StdTcpListener) -> Self {
        self.prebound_listeners.push(Some(listener));
        self
    }

    /// Supplies one pre-bound listener per worker.
    ///
    /// The vector index corresponds to the worker id; worker `i` will receive
    /// `listeners[i]`. This avoids races when multiple workers need contiguous
    /// ports in multi-port mode.
    pub fn with_prebound_listeners(mut self, listeners: Vec<StdTcpListener>) -> Self {
        self.prebound_listeners = listeners.into_iter().map(Some).collect();
        self
    }

    pub fn cfg(&self) -> &ServerCfg {
        &self.cfg
    }

    pub fn deps(&self) -> &ServerRuntimeDeps {
        &self.deps
    }

    /// Removes and returns the pre-bound listener for `worker_id`, if one was
    /// supplied.
    pub fn take_prebound_listener(&mut self, worker_id: usize) -> Option<StdTcpListener> {
        self.prebound_listeners
            .get_mut(worker_id)
            .and_then(Option::take)
    }
}

/// Alias used by backend construction code that does not need a transport-specific name.
pub type WorkerTcpBackendConfig = ServerLaunch;
