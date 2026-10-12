use crate::common::provider_type::ProviderType;
use crate::contract::async_io_provider::AsyncIoProvider;
use crate::SysIoContext;
use crate::{SysEnvVar, SysOs, SysProcess, SysRandom, SysSync, SysTasks, SysThread, SysTime};
use std::sync::{Arc, OnceLock};

/// System environment aggregate.
///
/// `Sys` is a facade over IO context and domain-specific subsystems:
/// `SysTime`, `SysRandom`, `SysThread`, `SysSync`, `SysTasks`, `SysEnvVar`,
/// `SysProcess`, `SysOs`.
/// Use the accessor methods (`time()`, `random()`, `thread()`, `sync()`,
/// `tasks()`, `env_var()`, `process()`, `os()`) to reach the subsystem implementations.
pub struct Sys {
    default_context: OnceLock<Arc<SysIoContext>>,
    tokio_context: Arc<SysIoContext>,
    time: SysTime,
    random: SysRandom,
    thread: SysThread,
    sync: SysSync,
    tasks: SysTasks,
    env_var: SysEnvVar,
    process: SysProcess,
    os: SysOs,
}

impl Default for Sys {
    fn default() -> Self {
        Self::new()
    }
}

impl Sys {
    /// Creates a new system environment with all subsystems initialized and
    /// the default IO context not yet selected (it falls back to Tokio until
    /// [`Sys::setup`] is called).
    pub fn new() -> Self {
        Self {
            default_context: OnceLock::new(),
            tokio_context: SysIoContext::tokio(),
            time: SysTime::new(),
            random: SysRandom::new(),
            thread: SysThread::new(),
            sync: SysSync::new(),
            tasks: SysTasks::new(),
            env_var: SysEnvVar::new(),
            process: SysProcess::new(),
            os: SysOs::new(),
        }
    }

    /// Selects the async IO provider used by the default IO context.
    ///
    /// Only the first call takes effect; later calls are silently ignored.
    pub fn setup(&self, provider_type: ProviderType) {
        let _ = self
            .default_context
            .set(SysIoContext::with_provider(provider_type));
    }

    /// Returns the default IO context, initializing it to the Tokio provider
    /// if [`Sys::setup`] has not selected another one.
    pub fn context(&self) -> &SysIoContext {
        self.default_context
            .get_or_init(SysIoContext::tokio)
            .as_ref()
    }

    /// Returns the async IO provider of the default context.
    pub fn provider(&self) -> &dyn AsyncIoProvider {
        self.context().provider()
    }

    /// Returns a shared handle to the async IO provider of the default
    /// context.
    pub fn provider_arc(&self) -> Arc<dyn AsyncIoProvider> {
        self.context().provider_arc()
    }

    /// Returns the dedicated Tokio IO context (always Tokio, regardless of
    /// the provider chosen via [`Sys::setup`]).
    pub fn tokio_context(&self) -> &SysIoContext {
        self.tokio_context.as_ref()
    }

    /// Returns the async IO provider of the dedicated Tokio context.
    pub fn tokio_provider(&self) -> &dyn AsyncIoProvider {
        self.tokio_context.provider()
    }

    /// Returns the time subsystem.
    pub fn time(&self) -> &SysTime {
        &self.time
    }

    /// Returns the random generation subsystem.
    pub fn random(&self) -> &SysRandom {
        &self.random
    }

    /// Returns the thread subsystem.
    pub fn thread(&self) -> &SysThread {
        &self.thread
    }

    /// Returns the synchronization subsystem.
    pub fn sync(&self) -> &SysSync {
        &self.sync
    }

    /// Returns the async task subsystem.
    pub fn tasks(&self) -> &SysTasks {
        &self.tasks
    }

    /// Returns the environment variable subsystem.
    pub fn env_var(&self) -> &SysEnvVar {
        &self.env_var
    }

    /// Returns the process subsystem.
    pub fn process(&self) -> &SysProcess {
        &self.process
    }

    /// Returns the OS information subsystem.
    pub fn os(&self) -> &SysOs {
        &self.os
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::contract::async_mode::AsyncMode;

    #[test]
    fn sys_new_and_default_construct() {
        let _ = Sys::new();
        let _ = Sys::default();
    }

    #[test]
    fn sys_accessors_return_non_null_subsystems() {
        let sys = Sys::new();
        let _ = sys.time().instant_now();
        let _ = sys.random().uuid_v4();
        let handle = sys.thread().spawn(|| 1).unwrap();
        assert_eq!(handle.join().unwrap(), 1);
        let mutex = sys.sync().mutex(0);
        *mutex.lock().unwrap() = 1;
        assert!(!sys.tasks().has_tokio_runtime());
        let _ = sys.env_var().temp_dir();
        let _ = sys.process();
        let _ = sys.os();
    }

    #[test]
    fn sys_context_defaults_to_tokio() {
        let sys = Sys::new();
        assert_eq!(sys.context().provider().mode(), AsyncMode::Tokio);
    }

    #[test]
    fn sys_tokio_context_and_provider_are_valid() {
        let sys = Sys::new();
        assert_eq!(sys.tokio_context().provider().mode(), AsyncMode::Tokio);
        assert_eq!(sys.tokio_provider().mode(), AsyncMode::Tokio);
    }

    #[test]
    fn sys_setup_tokio_succeeds_and_context_stays_tokio() {
        let sys = Sys::new();
        sys.setup(ProviderType::Tokio);
        assert_eq!(sys.context().provider().mode(), AsyncMode::Tokio);
    }

    #[test]
    fn sys_provider_arc_and_provider_point_to_same_object() {
        let sys = Sys::new();
        let arc = sys.provider_arc();
        let ptr = sys.provider() as *const dyn AsyncIoProvider as *const ();
        assert_eq!(Arc::as_ptr(&arc).cast::<()>(), ptr);
    }
}
