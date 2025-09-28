use crate::contract::pst_op_list::PstOpList;
use mudu::common::result::RS;

pub trait PstOpCh: Send + Sync {
    fn async_run(&self, ops: PstOpList) -> RS<()>;
}
