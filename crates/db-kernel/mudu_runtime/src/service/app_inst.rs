use async_trait::async_trait;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu_contract::database::sql::DBConn;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_utils::task_id::TaskID;
use std::sync::Arc;

/// Trait implemented by a loaded Mudu application instance.
#[async_trait]
pub trait AppInst: Send + Sync {
    /// Returns the application configuration.
    fn cfg(&self) -> &AppInfo;

    /// Creates a new task context and returns its identifier.
    async fn task_create(&self) -> RS<TaskID>;

    /// Ends the task with the given identifier.
    fn task_end(&self, task_id: TaskID) -> RS<()>;

    /// Returns the database connection associated with a task, if any.
    fn connection(&self, task_id: TaskID) -> Option<DBConn>;

    /// Lists the procedures exposed by this application.
    fn procedure(&self) -> RS<Vec<(String, String)>>;

    /// Invokes a synchronous procedure.
    async fn invoke(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult>;

    /// Invokes an asynchronous procedure.
    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &str,
        proc_name: &str,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult>;

    /// Returns the descriptor for the named procedure.
    fn describe(&self, mod_name: &str, proc_name: &str) -> RS<Arc<ProcDesc>>;
}
