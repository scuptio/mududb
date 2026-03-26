use crate::service::service_trait::ServiceTrait;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::sync::async_task::TaskWrapper;
use tracing::info;

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
        let mut builder = tokio::runtime::Builder::new_current_thread();
        let r = builder
            .enable_all()
            .build()
            .map_err(|e| m_error!(EC::IOErr, "build runtime error", e))?
            .block_on(async {
                let mut task_result = vec![];
                let mut result = vec![];
                let mut joinable = vec![];
                while let Some(task) = tasks.pop() {
                    let join_handle = task.as_ref().async_run();
                    task_result.push(join_handle);
                }
                result.resize_with(task_result.len(), || Some(m_error!(EC::NoneErr)));
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
                    Ok(result)
                } else {
                    TaskWrapper::join_all(joinable)
                        .await
                        .map_err(|e| m_error!(EC::IOErr, "join error", e))?;
                    Ok(result)
                }
            })?;
        info!("task join result: {:?}", r);
        Ok(())
    }
}
