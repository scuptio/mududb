use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::database::sql::DBConn;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_utils::task_id::TaskID;
use std::sync::Arc;
use mudu::common::package_cfg::PackageCfg;

#[async_trait]
pub trait AppInst: Send + Sync {

    fn cfg(&self) -> &PackageCfg;
    
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
    ) -> RS<ProcedureResult>;

    async fn invoke_async(
        &self,
        task_id: TaskID,
        mod_name: &String,
        proc_name: &String,
        param: ProcedureParam,
    ) -> RS<ProcedureResult>;

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>>;
}
