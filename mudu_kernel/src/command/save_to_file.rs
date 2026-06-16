use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::table_desc::TableDesc;
use crate::x_engine::api::{OptRead, Predicate, RangeData, VecSelTerm, XContract};
use crate::x_engine::tx_mgr::TxMgr;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::sync::async_::AMutex;
use std::ops::Bound;
use std::path::Path;
use std::sync::Arc;

pub struct SaveToFile {
    inner: AMutex<_SaveToFile>,
}

pub struct SaveToFileParams {
    pub file_path: String,
    pub tx_mgr: Arc<dyn TxMgr>,
    pub table_id: OID,
    pub key_indexing: Vec<usize>,
    pub value_indexing: Vec<usize>,
    pub x_contract: Arc<dyn XContract>,
    pub meta_mgr: Arc<dyn MetaMgr>,
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}

struct _SaveToFile {
    file_path: String,
    tx_mgr: Arc<dyn TxMgr>,
    table_id: OID,
    key_indexing: Vec<usize>,
    value_indexing: Vec<usize>,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    affected_rows: u64,
}

impl SaveToFile {
    pub fn new(params: SaveToFileParams) -> Self {
        Self {
            inner: AMutex::new(_SaveToFile::new(params)),
        }
    }
}

#[async_trait]
impl CmdExec for SaveToFile {
    async fn prepare(&self) -> RS<()> {
        let inner = self.inner.lock().await;
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        let mut inner = self.inner.lock().await;
        let rows = inner.save_table().await?;
        inner.affected_rows = rows;
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        let inner = self.inner.lock().await;
        Ok(inner.affected_rows)
    }
}

impl _SaveToFile {
    fn new(params: SaveToFileParams) -> Self {
        Self {
            file_path: params.file_path,
            tx_mgr: params.tx_mgr,
            table_id: params.table_id,
            key_indexing: params.key_indexing,
            value_indexing: params.value_indexing,
            x_contract: params.x_contract,
            meta_mgr: params.meta_mgr,
            async_runtime: params.async_runtime,
            affected_rows: 0,
        }
    }

    async fn prepare(&self) -> RS<()> {
        let table_desc = self.meta_mgr.get_table_by_id(self.table_id).await?;
        if self.key_indexing.len() != table_desc.key_info().len()
            || self.value_indexing.len() != table_desc.value_info().len()
        {
            return Err(m_error!(ER::IOErr, "column size error"));
        }
        let total = self.key_indexing.len() + self.value_indexing.len();
        Self::validate_indexing(&self.key_indexing, &self.value_indexing, total)
    }

    async fn save_table(&self) -> RS<u64> {
        let table_desc = self.meta_mgr.get_table_by_id(self.table_id).await?;
        let select = Self::build_select(&table_desc);
        let output_desc = Self::build_output_desc(&table_desc);
        let cursor = self
            .x_contract
            .read_range(
                self.tx_mgr.clone(),
                self.table_id,
                &RangeData::new(Bound::Unbounded, Bound::Unbounded),
                &Predicate::CNF(Vec::new()),
                &select,
                &OptRead::default(),
            )
            .await?;

        let mut writer = csv::WriterBuilder::new()
            .has_headers(false)
            .from_writer(Vec::new());
        let header = self.reorder_row(&Self::build_header(&table_desc))?;
        writer.write_record(header).map_err(|e| {
            m_error!(
                ER::IOErr,
                format!(
                    "save failed, write csv header {} error, {}",
                    self.file_path, e
                )
            )
        })?;

        let mut rows = 0;
        while let Some(row) = cursor.next().await? {
            let textual = row.to_textual(&output_desc)?;
            let ordered = self.reorder_row(&textual)?;
            writer.write_record(ordered).map_err(|e| {
                m_error!(
                    ER::IOErr,
                    format!("save failed, write csv row {} error, {}", self.file_path, e)
                )
            })?;
            rows += 1;
        }
        writer.flush().map_err(|e| {
            m_error!(
                ER::IOErr,
                format!(
                    "save failed, flush csv file {} error, {}",
                    self.file_path, e
                )
            )
        })?;

        let payload = writer.into_inner().map_err(|e| {
            m_error!(
                ER::IOErr,
                format!(
                    "save failed, finalize csv writer {} error, {}",
                    self.file_path, e
                )
            )
        })?;
        let file_path = normalized_copy_path(&self.file_path);
        let async_runtime = self
            .async_runtime
            .as_ref()
            .ok_or_else(|| m_error!(ER::IOErr, "save failed, no async runtime available"))?;
        write_csv_payload(async_runtime, &file_path, payload).await?;
        Ok(rows)
    }

    fn validate_indexing(key_indexing: &[usize], value_indexing: &[usize], total: usize) -> RS<()> {
        let mut seen = vec![false; total];
        for idx in key_indexing.iter().chain(value_indexing.iter()) {
            if *idx >= total {
                return Err(m_error!(ER::IndexOutOfRange));
            }
            if seen[*idx] {
                return Err(m_error!(ER::IOErr, "duplicate column index"));
            }
            seen[*idx] = true;
        }
        if seen.iter().any(|item| !item) {
            return Err(m_error!(ER::IOErr, "column index is not continuous"));
        }
        Ok(())
    }

    fn build_select(table_desc: &TableDesc) -> VecSelTerm {
        let total = table_desc.fields().len();
        VecSelTerm::new((0..total).collect())
    }

    fn build_output_desc(table_desc: &TableDesc) -> Vec<DatumDesc> {
        let total = table_desc.fields().len();
        (0..total)
            .map(|attr| {
                let field = table_desc.get_attr(attr);
                DatumDesc::new(field.name().clone(), field.type_desc().clone())
            })
            .collect()
    }

    fn build_header(table_desc: &TableDesc) -> Vec<String> {
        let total = table_desc.fields().len();
        (0..total)
            .map(|attr| table_desc.get_attr(attr).name().clone())
            .collect()
    }

    fn reorder_row(&self, textual: &[String]) -> RS<Vec<String>> {
        let total = self.key_indexing.len() + self.value_indexing.len();
        if textual.len() != total {
            return Err(m_error!(ER::IOErr, "row column size error"));
        }
        let mut ordered = vec![String::new(); total];
        for (src, dest) in self.key_indexing.iter().enumerate().chain(
            self.value_indexing
                .iter()
                .enumerate()
                .map(|(i, idx)| (self.key_indexing.len() + i, idx)),
        ) {
            ordered[*dest] = textual[src].clone();
        }
        Ok(ordered)
    }
}

async fn write_csv_payload(
    async_runtime: &Arc<dyn AsyncIoProvider>,
    path: &str,
    payload: Vec<u8>,
) -> RS<()> {
    let fs = async_runtime.fs_arc();
    let file = fs
        .open(
            Path::new(path),
            FileOptions::new(
                libc::O_CREAT | libc::O_TRUNC | libc::O_WRONLY | libc::O_CLOEXEC,
                0o644,
            ),
        )
        .await
        .map_err(|e| {
            m_error!(
                ER::IOErr,
                format!("save failed, open csv file {} error, {}", path, e)
            )
        })?;
    let write_result = file.write_all_at(0, &payload).await.map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("save failed, write csv file {} error, {}", path, e)
        )
    });
    let flush_result = file.fsync().await;
    let close_result = file.close().await;
    write_result?;
    flush_result.map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("save failed, flush csv file {} error, {}", path, e)
        )
    })?;
    close_result.map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("save failed, close csv file {} error, {}", path, e)
        )
    })?;
    Ok(())
}

fn normalized_copy_path(path: &str) -> String {
    let bytes = path.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\'')
            || (bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"'))
    {
        return path[1..path.len() - 1].to_string();
    }
    path.to_string()
}
