use crate::service::service_impl::ServiceImpl;
use crate::service::service_trait::ServiceTrait;
use mudu::common::result::RS;
use mudu_utils::sync::async_task::TaskWrapper;

pub struct Service {
    service: ServiceImpl,
}

impl Service {
    pub fn new() -> Self {
        Self {
            service: ServiceImpl::new(),
        }
    }

    pub fn register(&self, task: TaskWrapper) -> RS<()> {
        self.service.register(task)
    }

    pub fn serve(self) -> RS<()> {
        self.service.serve()
    }
}
