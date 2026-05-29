use crate::contract::cmd_exec::CmdExec;
use crate::contract::meta_mgr::MetaMgr;
use crate::io::file as async_file;
use crate::x_engine::api::{OptInsert, VecDatum, XContract};
use crate::x_engine::tx_mgr::TxMgr;
use async_trait::async_trait;
use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_type::dat_type_id::DatTypeID;
use mudu_utils::scoped_task_trace;
use mudu_utils::sync::a_mutex::AMutex;
use std::io::Cursor;
use std::sync::Arc;
use tracing::debug;

pub struct LoadFromFile {
    inner: Arc<AMutex<_LoadFromFile>>,
}

struct _LoadFromFile {
    csv_file: String,
    tx_mgr: Arc<dyn TxMgr>,
    table_id: OID,
    key_index: Vec<usize>,
    value_index: Vec<usize>,
    x_contract: Arc<dyn XContract>,
    meta_mgr: Arc<dyn MetaMgr>,
    affected_rows: u64,
}

impl LoadFromFile {
    pub fn new(
        csv_file: String,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        key_index: Vec<usize>,
        value_index: Vec<usize>,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            inner: Arc::new(AMutex::new(_LoadFromFile::new(
                csv_file,
                tx_mgr,
                table_id,
                key_index,
                value_index,
                x_contract,
                meta_mgr,
            ))),
        }
    }
}

impl _LoadFromFile {
    fn new(
        csv_file: String,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        key_index: Vec<usize>,
        value_index: Vec<usize>,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> Self {
        Self {
            csv_file,
            tx_mgr,
            table_id,
            key_index,
            value_index,
            x_contract,
            meta_mgr,
            affected_rows: 0,
        }
    }

    async fn prepare(&self) -> RS<()> {
        debug!(
            table_id = self.table_id,
            csv_file = %self.csv_file,
            key_cols = self.key_index.len(),
            value_cols = self.value_index.len(),
            "copy from prepare"
        );
        let table_desc = self.meta_mgr.get_table_by_id(self.table_id).await?;
        if self.key_index.len() != table_desc.key_info().len()
            || self.value_index.len() != table_desc.value_info().len()
        {
            return Err(m_error!(ER::IOErr, "column size error"));
        }
        Ok(())
    }

    async fn load_table(&self) -> RS<u64> {
        scoped_task_trace!();
        debug!(
            table_id = self.table_id,
            csv_file = %self.csv_file,
            "copy from start loading"
        );
        let table_desc = self.meta_mgr.get_table_by_id(self.table_id).await?;
        let csv_path = normalized_copy_path(&self.csv_file);
        let payload = read_csv_payload(&csv_path).await?;

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(Cursor::new(payload));
        let mut rows = 0;
        for record in reader.records() {
            debug!(
                table_id = self.table_id,
                csv_file = %self.csv_file,
                rows,
                "copy from waiting next csv record"
            );
            let record = record.map_err(|e| {
                m_error!(
                    ER::IOErr,
                    format!("load failed, csv file {} error, {}", self.csv_file, e)
                )
            })?;
            let field_num = self.key_index.len() + self.value_index.len();
            if field_num != record.len() {
                return Err(m_error!(
                    ER::IOErr,
                    format!(
                        "load failed, table column size {} not equal to csv column count {}",
                        field_num,
                        record.len()
                    )
                ));
            }

            let key = Self::build_datum_from_line(
                &record,
                &self.key_index,
                table_desc.key_indices(),
                &table_desc,
            )?;
            let value = Self::build_datum_from_line(
                &record,
                &self.value_index,
                table_desc.value_indices(),
                &table_desc,
            )?;
            self.x_contract
                .insert(
                    self.tx_mgr.clone(),
                    self.table_id,
                    &key,
                    &value,
                    &OptInsert::default(),
                )
                .await?;
            rows += 1;
            debug!(
                table_id = self.table_id,
                csv_file = %self.csv_file,
                rows,
                "copy from inserted row"
            );
        }
        debug!(
            table_id = self.table_id,
            csv_file = %self.csv_file,
            rows,
            "copy from csv exhausted"
        );
        debug!(
            table_id = self.table_id,
            csv_file = %self.csv_file,
            rows,
            "copy from finished loading"
        );
        Ok(rows)
    }

    fn set_affected_rows(&mut self, rows: u64) {
        self.affected_rows = rows;
    }

    fn get_affected_rows(&self) -> u64 {
        self.affected_rows
    }

    fn build_datum_from_line(
        record: &csv::StringRecord,
        csv_index: &[usize],
        attr_indices: &[usize],
        table_desc: &crate::contract::table_desc::TableDesc,
    ) -> RS<VecDatum> {
        let mut datum = Vec::with_capacity(csv_index.len());
        for (position, csv_col) in csv_index.iter().enumerate() {
            let textual = record
                .get(*csv_col)
                .ok_or_else(|| m_error!(ER::IndexOutOfRange))?;
            let attr_index = attr_indices[position];
            let field = table_desc.get_attr(attr_index);
            let dat_type = field.type_desc();
            let dat_id = dat_type.dat_type_id();
            let internal = match dat_id.fn_input()(textual, dat_type) {
                Ok(internal) => internal,
                Err(first_err) => {
                    if dat_id == DatTypeID::String {
                        // COPY FROM accepts both JSON textual strings ("Alice")
                        // and plain CSV cells (Alice) for string columns.
                        let quoted = serde_json::to_string(textual).map_err(|e| {
                            m_error!(ER::TypeBaseErr, "convert printable to internal error", e)
                        })?;
                        dat_id.fn_input()(&quoted, dat_type).map_err(|_| {
                            m_error!(
                                ER::TypeBaseErr,
                                "convert printable to internal error",
                                first_err
                            )
                        })?
                    } else {
                        return Err(m_error!(
                            ER::TypeBaseErr,
                            "convert printable to internal error",
                            first_err
                        ));
                    }
                }
            };
            let binary: Buf = dat_id.fn_send()(&internal, dat_type)
                .map_err(|e| m_error!(ER::TypeBaseErr, "converting internal to binary error", e))?
                .into();
            datum.push((attr_index, binary));
        }
        Ok(VecDatum::new(datum))
    }
}

async fn read_csv_payload(path: &str) -> RS<Vec<u8>> {
    // Even on the io_uring backend, worker threads run inside a Tokio runtime.
    // Routing COPY FROM through `mudu_sys::tokio::fs` here would silently bypass the
    // worker-ring/io_uring file path and reintroduce the hang we fixed.
    // Always use the kernel async file abstraction instead.
    let file = async_file::open(path, libc::O_RDONLY | async_file::cloexec_flag(), 0)
        .await
        .map_err(|e| {
            m_error!(
                ER::IOErr,
                format!("load failed, open csv file {} error, {}", path, e)
            )
        })?;
    // Query the length from the opened fd so COPY FROM does not depend on a
    // second path-based metadata lookup after open succeeds.
    let len = async_file::metadata_len_by_file(&file).map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("load failed, stat csv file {} error, {}", path, e)
        )
    })? as usize;
    let result = async_file::read(&file, len, 0).await.map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("load failed, read csv file {} error, {}", path, e)
        )
    });
    let close_result = async_file::close(file).await;
    let payload = result?;
    close_result.map_err(|e| {
        m_error!(
            ER::IOErr,
            format!("load failed, close csv file {} error, {}", path, e)
        )
    })?;
    Ok(payload)
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

#[async_trait]
impl CmdExec for LoadFromFile {
    async fn prepare(&self) -> RS<()> {
        let inner = self.inner.lock().await;
        inner.prepare().await
    }

    async fn run(&self) -> RS<()> {
        scoped_task_trace!();
        let mut inner = self.inner.lock().await;
        let rows = inner.load_table().await?;
        inner.set_affected_rows(rows);
        Ok(())
    }

    async fn affected_rows(&self) -> RS<u64> {
        let inner = self.inner.lock().await;
        Ok(inner.get_affected_rows())
    }
}
