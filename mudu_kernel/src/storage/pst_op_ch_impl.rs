use crate::contract::pst_op_list::PstOpList;
use crate::storage::pst_op_ch::PstOpCh;
use crate::storage::pst_store_impl::PstOpChImpl;
use mudu::common::result::RS;

impl PstOpCh for PstOpChImpl {
    fn async_run(&self, ops: PstOpList) -> RS<()> {
        self.async_run_ops(ops.into_ops())
    }
}
