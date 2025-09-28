use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use tokio::sync::oneshot::Sender;

pub struct InsertKV {
    pub table_id: OID,
    pub tuple_id: OID,
    pub timestamp: Timestamp,
    pub key: Buf,
    pub value: Buf,
}

pub struct UpdateV {
    pub table_id: OID,
    pub tuple_id: OID,
    pub timestamp: Timestamp,
    pub value: Buf,
}

pub struct DeleteKV {
    pub table_id: OID,
    pub tuple_id: OID,
}

pub struct WriteDelta {
    pub table_id: OID,
    pub tuple_id: OID,
    pub timestamp: Timestamp,
    pub delta: Buf,
}

pub enum PstOp {
    InsertKV(InsertKV),
    UpdateV(UpdateV),
    DeleteKV(DeleteKV),
    WriteDelta(WriteDelta),
    Flush(Sender<()>),
    Stop(Sender<()>),
}
