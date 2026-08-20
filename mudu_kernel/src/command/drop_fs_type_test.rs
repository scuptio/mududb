#![allow(clippy::unwrap_used)]
use crate::command::drop_fs_type::DropFsType;
use crate::contract::cmd_exec::CmdExec;
use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::x_engine::x_param::PDropType;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::Arc;

fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

struct MockMetaMgr {
    fs_types: SMutex<HashMap<String, FsTypeDesc>>,
    next_fs_id: SMutex<u64>,
}

impl MockMetaMgr {
    fn new() -> Self {
        Self {
            fs_types: SMutex::new(HashMap::new()),
            next_fs_id: SMutex::new(1),
        }
    }

    fn get_fs_type(&self, name: &str) -> Option<FsTypeDesc> {
        self.fs_types.lock().unwrap().get(name).cloned()
    }
}

#[async_trait]
impl MetaMgr for MockMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, oid: OID) -> RS<Arc<TableDesc>> {
        Err(mudu::mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such table {}", oid)
        ))
    }

    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }

    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }

    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }

    async fn create_fs_type(&self, name: &str, kind: FsTypeKind) -> RS<u64> {
        let mut next_fs_id = self.next_fs_id.lock().unwrap();
        let fs_id = *next_fs_id;
        *next_fs_id += 1;
        self.fs_types.lock().unwrap().insert(
            name.to_string(),
            FsTypeDesc::new(name.to_string(), fs_id, kind),
        );
        Ok(fs_id)
    }

    async fn get_fs_type_by_name(&self, name: &str) -> RS<Option<FsTypeDesc>> {
        Ok(self.fs_types.lock().unwrap().get(name).cloned())
    }

    async fn drop_fs_type(&self, name: &str) -> RS<()> {
        self.fs_types.lock().unwrap().remove(name);
        Ok(())
    }
}

#[test]
fn drop_fs_type_removes_registered_type() {
    let meta = Arc::new(MockMetaMgr::new());
    block_on(meta.create_fs_type("wal_log", FsTypeKind::File)).unwrap();

    let cmd = DropFsType::new(
        PDropType {
            name: "wal_log".to_string(),
        },
        meta.clone(),
    );
    block_on(cmd.prepare()).unwrap();
    block_on(cmd.run()).unwrap();
    assert_eq!(block_on(cmd.affected_rows()).unwrap(), 0);
    assert!(meta.get_fs_type("wal_log").is_none());
}

#[test]
fn drop_fs_type_prepare_fails_on_unknown_name() {
    let meta = Arc::new(MockMetaMgr::new());
    let cmd = DropFsType::new(
        PDropType {
            name: "no_such_type".to_string(),
        },
        meta,
    );
    let err = block_on(cmd.prepare()).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}
