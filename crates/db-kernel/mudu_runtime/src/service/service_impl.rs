use crate::service::service_trait::ServiceTrait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_sys::sync::async_::async_task::TaskWrapper;
use mudu_utils::task_async::build_current_thread_runtime;
use tracing::debug;

pub struct ServiceImpl {
    tasks: scc::Queue<TaskWrapper>,
}

impl ServiceImpl {
    pub fn new() -> Self {
        Self {
            tasks: Default::default(),
        }
    }
}

impl ServiceTrait for ServiceImpl {
    fn register(&self, task: TaskWrapper) -> RS<()> {
        self.tasks.push(task);
        Ok(())
    }

    fn serve(self) -> RS<()> {
        let tasks = self.tasks;
        let r = build_current_thread_runtime()?.block_on(async {
            let mut task_result = vec![];
            let mut result = vec![];
            let mut joinable = vec![];
            while let Some(task) = tasks.pop() {
                let join_handle = task.as_ref().async_run();
                task_result.push(join_handle);
            }
            result.resize_with(task_result.len(), || {
                Some(mudu_error!(ErrorCode::InvalidState))
            });
            let mut error_count = 0;
            for (i, join) in task_result.into_iter().enumerate() {
                match join {
                    Ok(r) => joinable.push(r),
                    Err(e) => {
                        error_count += 1;
                        result[i] = Some(e);
                    }
                }
            }
            if error_count > 0 {
                Ok::<_, MuduError>(result)
            } else {
                TaskWrapper::join_all(joinable).await?;
                Ok::<_, MuduError>(result)
            }
        })?;
        debug!("task join result: {:?}", r);
        Ok(())
    }
}
