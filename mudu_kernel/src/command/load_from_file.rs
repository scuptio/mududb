use crate::contract::cmd_exec::CmdExec;
use crate::contract::data_row::DataRow;
use crate::contract::pst_op_list::PstOpList;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::x_engine::thd_ctx::ThdCtx;
use async_std::fs::File;
use async_trait::async_trait;
use csv_async::StringRecord;
use futures::StreamExt;
use mudu::common::buf::Buf;
use mudu::common::id::{gen_oid, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu::tuple::build_tuple::build_tuple;
use mudu::tuple::dat_printable::DatPrintable;
use mudu::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use std::sync::Arc;
use tokio::sync::oneshot::channel;
use tokio::sync::Mutex;

pub struct LoadFromFile {
    inner: Arc<Mutex<_LoadFromFile>>,
}

struct _LoadFromFile {
    csv_file: String,
    table_id: OID,
    key_index: Vec<usize>,
    value_index: Vec<usize>,
    key_desc: TupleDesc,
    value_desc: TupleDesc,
    thd_ctx: ThdCtx,
    affected_rows: u64,
}

impl LoadFromFile {
    pub fn new(
        csv_file: String,
        table_id: OID,
        key_index: Vec<usize>,
        value_index: Vec<usize>,
        key_desc: TupleDesc,
        value_desc: TupleDesc,
        thd_ctx: ThdCtx,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(_LoadFromFile::new(
                csv_file,
                table_id,
                key_index,
                value_index,
                key_desc,
                value_desc,
                thd_ctx,
            ))),
        }
    }
}

impl _LoadFromFile {
    fn new(
        csv_file: String,
        table_id: OID,
        key_index: Vec<usize>,
        value_index: Vec<usize>,
        key_desc: TupleDesc,
        value_desc: TupleDesc,
        thd_ctx: ThdCtx,
    ) -> Self {
        if key_index.len() != key_desc.field_count()
            || value_index.len() != value_desc.field_count()
        {
            panic!("column size error!");
        }
        Self {
            csv_file,
            table_id,
            key_index,
            value_index,
            key_desc,
            value_desc,
            thd_ctx,
            affected_rows: 0,
        }
    }

    async fn load_table(&self) -> RS<u64> {
        let file = File::open(self.csv_file.clone()).await.map_err(|e| {
            m_error!(ER::IOErr, format!(
                "load failed, open csv file {} error, {}",
                self.csv_file, e
            ))
        })?;
        let mut rdr = csv_async::AsyncReader::from_reader(file);
        let mut records = rdr.records();
        let mut rows = 0;
        while let Some(t) = records.next().await {
            let record = t.map_err(|e| {
                m_error!(ER::IOErr, format!(
                    "load failed, csv file {} error, {}",
                    self.csv_file, e
                ))
            })?;
            let field_num = self.key_index.len() + self.value_index.len();
            if field_num != record.len() {
                return Err(m_error!(ER::IOErr, format!(
                    "load failed, table column size {} not equal to csv column count {}",
                    field_num,
                    record.len()
                )));
            }
            let key = Self::build_tuple_from_line(&record, &self.key_index, &self.key_desc)?;
            let value = Self::build_tuple_from_line(&record, &self.value_index, &self.value_desc)?;
            let data_row = DataRow::new();
            let opt = self
                .thd_ctx
                .tree_store()
                .insert_key(self.table_id, key.clone(), data_row.clone())
                .await?;
            let data_row = match opt {
                Some((_k, row)) => row,
                None => data_row,
            };
            let timestamp = Timestamp::default();
            let tuple = VersionTuple::new(timestamp.clone(), value.clone());
            data_row.write(tuple, None).await?;
            let mut list = PstOpList::new();
            list.push_insert(self.table_id, gen_oid(), timestamp, key, value);
            self.thd_ctx.pst_op_ch().async_run(list)?;
            rows += 1;
        }
        let (s, r) = channel();
        let mut list = PstOpList::new();
        list.push_flush(s);
        self.thd_ctx.pst_op_ch().async_run(list)?;
        r.await
            .map_err(|e| m_error!(ER::IOErr, "flush failed", e))?;
        Ok(rows)
    }

    fn set_affected_rows(&mut self, rows: u64) {
        self.affected_rows = rows;
    }

    fn get_affected_rows(&self) -> u64 {
        self.affected_rows
    }

    fn build_tuple_from_line(
        record: &StringRecord,
        index: &[usize],
        tuple_desc: &TupleDesc,
    ) -> RS<Buf> {
        let mut tuple = vec![];
        for i in index.iter() {
            let opt = record.get(*i);
            let s = match opt {
                Some(s) => s.to_string(),
                None => return Err(m_error!(ER::IndexOutOfRange)),
            };
            let field_desc = &tuple_desc.field_desc()[*i];
            let dat_id = field_desc.data_type();
            let type_param = field_desc.type_param();
            let internal = dat_id.fn_input()(&DatPrintable::from(s), type_param)
                .map_err(|e| {
                    m_error!(ER::ConvertErr, "convert printable to internal error", e)
                })?;
            let binary = dat_id.fn_send()(&internal, type_param)
                .map_err(|e| {
                    m_error!(ER::ConvertErr, "converting internal to binary error", e)
                })?;
            tuple.push(binary.into());
        }
        let buf = build_tuple(&tuple, tuple_desc)?;
        Ok(buf)
    }
}

#[async_trait]
impl CmdExec for LoadFromFile {
    async fn prepare(&self) -> RS<()> {
        Ok(())
    }

    async fn run(&self) -> RS<()> {
        let mut g = self.inner.lock().await;
        let rows = g.load_table().await?;
        g.set_affected_rows(rows);
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        let g = self.inner.lock().await;
        let rows = g.get_affected_rows();
        Ok(rows)
    }
}
