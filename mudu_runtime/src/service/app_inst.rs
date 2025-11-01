use mudu::common::result::RS;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use std::sync::Arc;
use mudu_utils::task_id::TaskID;

pub trait AppInst: Send + Sync {
    fn task_create(&self) -> RS<TaskID>;

    fn task_end(&self, task_id: TaskID) -> RS<()>;

    fn procedure(&self) -> RS<Vec<(String, String)>>;

    fn invoke(&self, task_id:TaskID, mod_name: &String, proc_name: &String, param: ProcParam) -> RS<ProcResult>;

    fn describe(&self, mod_name: &String, proc_name: &String) -> RS<Arc<ProcDesc>>;
}
