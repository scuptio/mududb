use mudu::common::result::RS;
use mudu_sys::sync::async_::async_task::TaskWrapper;

pub trait ServiceTrait: Send + Sync + 'static {
    fn register(&self, task: TaskWrapper) -> RS<()>;

    fn serve(self) -> RS<()>;
}
