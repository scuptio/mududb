use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::sql::Context;
use mudu::database::tx::Tx;
#[cfg(not(target_arch = "wasm32"))]
use postgres::Transaction;
use std::mem::ManuallyDrop;

pub struct TxPg {
    xid: XID,
    transaction: ManuallyDrop<Transaction<'static>>,
}

unsafe fn extend_lifetime<'b>(r: Transaction<'b>) -> Transaction<'static> {
    unsafe { std::mem::transmute::<Transaction<'b>, Transaction<'static>>(r) }
}

impl TxPg {
    pub fn new<'a>(conn: Transaction<'a>, xid: XID) -> Self {
        unsafe {
            Self {
                xid,
                transaction: ManuallyDrop::new(extend_lifetime(conn)),
            }
        }
    }

    pub fn commit(mut self) -> RS<()> {
        let t = unsafe { ManuallyDrop::take(&mut self.transaction) };
        t.commit().unwrap();
        Ok(())
    }

    pub fn rollback(mut self) -> RS<()> {
        let t = unsafe { ManuallyDrop::take(&mut self.transaction) };
        t.rollback().unwrap();
        Ok(())
    }

    pub fn transaction(&mut self) -> &mut Transaction<'static> {
        &mut self.transaction
    }
}

impl Tx for TxPg {
    fn xid(&self) -> XID {
        self.xid
    }
}

impl Drop for TxPg {
    fn drop(&mut self) {
        unsafe {
            ManuallyDrop::drop(&mut self.transaction);
        }
        let _ = Context::remove(self.xid);
    }
}
