use crate::service::service_impl::ServiceImpl;
use crate::service::service_trait::ServiceTrait;
use mudu::common::result::RS;
use mudu_sys::sync::async_::async_task::TaskWrapper;

/// Registry of async tasks that make up a backend service.
pub struct Service {
    service: ServiceImpl,
}

impl Default for Service {
    fn default() -> Self {
        Self::new()
    }
}

impl Service {
    /// Creates a new empty service registry.
    pub fn new() -> Self {
        Self {
            service: ServiceImpl::new(),
        }
    }

    /// Registers an async task with the service.
    pub fn register(&self, task: TaskWrapper) -> RS<()> {
        self.service.register(task)
    }

    /// Starts all registered tasks and waits for completion.
    pub fn serve(self) -> RS<()> {
        self.service.serve()
    }
}
