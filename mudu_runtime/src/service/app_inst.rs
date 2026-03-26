use async_trait::async_trait;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu_contract::database::sql::DBConn;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_kernel::server_ur::worker_local::WorkerLocalRef;
use mudu_utils::task_id::TaskID;
use std::sync::Arc;

#[async_trait]
pub trait AppInst: Send + Sync {
    fn cfg(&self) -> &AppInfo;

    async fn task_create(&self) -> RS<TaskID>;

    fn task_end(&self, task_id: TaskID) -> RS<()>;

    fn connection(&self, task_id: TaskID) -> Option<DBConn>;

    fn procedure(&self) -> RS<Vec<(String, String)>>;

    async fn invoke(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult>;

    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
        worker_local: Option<WorkerLocalRef>,
    ) -> RS<ProcedureResult>;

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>>;
}
