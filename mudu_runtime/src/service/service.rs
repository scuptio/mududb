use mudu::common::result::RS;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use std::sync::Arc;

pub trait Service: Send + Sync {
    fn invoke(&self, name: &String, param: ProcParam) -> RS<ProcResult>;
    fn describe(&self, name: &String) -> RS<Arc<ProcDesc>>;
}