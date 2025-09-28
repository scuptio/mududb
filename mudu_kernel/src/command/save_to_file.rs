use crate::contract::cmd_exec::CmdExec;
use crate::sync::a_mutex::AMutex;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;

pub struct SaveToFile {
    inner: AMutex<_SaveToFile>,
}

struct _SaveToFile {
    file_path: String,
    table_name: OID,
    key_indexing: Vec<usize>,
    value_indexing: Vec<usize>,
}

#[async_trait]
impl CmdExec for SaveToFile {
    async fn prepare(&self) -> RS<()> {
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        todo!()
    }
}

impl _SaveToFile {}
