use crate::contract::pst_op::{DeleteKV, InsertKV, PstOp, UpdateV};
use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use tokio::sync::oneshot::Sender;

impl PstOpList {
    pub fn new() -> PstOpList {
        Self { ops: Vec::new() }
    }

    pub fn into_ops(self) -> Vec<PstOp> {
        self.ops
    }

    pub fn push_delete(&mut self, table_id: OID, tuple_id: OID) {
        self.ops
            .push(PstOp::DeleteKV(DeleteKV { table_id, tuple_id }));
    }

    pub fn push_insert(
        &mut self,
        table_id: OID,
        tuple_id: OID,
        timestamp: Timestamp,
        key: Buf,
        value: Buf,
    ) {
        let op = InsertKV {
            table_id,
            tuple_id,
            timestamp,
            key,
            value,
        };
        self.ops.push(PstOp::InsertKV(op))
    }

    pub fn push_update(&mut self, table_id: OID, tuple_id: OID, timestamp: Timestamp, value: Buf) {
        let op = UpdateV {
            table_id,
            tuple_id,
            timestamp,
            value,
        };
        self.ops.push(PstOp::UpdateV(op))
    }

    pub fn push_stop(&mut self, sender: Sender<()>) {
        self.ops.push(PstOp::Stop(sender))
    }

    pub fn push_flush(&mut self, sender: Sender<()>) {
        self.ops.push(PstOp::Flush(sender))
    }
}

pub struct PstOpList {
    ops: Vec<PstOp>,
}
