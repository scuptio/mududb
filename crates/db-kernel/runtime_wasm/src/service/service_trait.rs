use mudu::common::result::RS;
use mudu_sys::sync::async_::async_task::TaskWrapper;

/// Behavior of a service task registry.
pub trait ServiceTrait: Send + Sync + 'static {
    /// Registers an async task with the service.
    fn register(&self, task: TaskWrapper) -> RS<()>;

    /// Starts all registered tasks and waits for completion.
    fn serve(self) -> RS<()>;
}
