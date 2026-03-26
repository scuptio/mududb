use mudu::common::result::RS;
use mudu_utils::sync::async_task::TaskWrapper;

pub trait ServiceTrait: Send + Sync + 'static {
    fn register(&self, task: TaskWrapper) -> RS<()>;

    fn serve(self) -> RS<()>;
}
