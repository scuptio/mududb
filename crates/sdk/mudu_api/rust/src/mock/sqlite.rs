use crate::error::ApiError;
use crate::types::{UniCommandResult, UniCommandReturn, UniQueryReturn};
use crate::{
    UniCommandArgv, UniDataType, UniDataValue, UniError, UniQueryArgv, UniQueryResult,
    UniRecordField, UniRecordType, UniResultSet, UniScalar, UniScalarValue, UniTupleRow,
};
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Connection, params_from_iter};
use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};

static DATABASE_PATH_OVERRIDE: OnceLock<RwLock<Option<PathBuf>>> = OnceLock::new();

pub struct MockSqliteMuduSysCall;

impl MockSqliteMuduSysCall {
    pub fn set_database_path(path: impl Into<PathBuf>) {
        let lock = DATABASE_PATH_OVERRIDE.get_or_init(|| RwLock::new(None));
        *lock.write().expect("database path lock poisoned") = Some(path.into());
    }

    pub fn database_path() -> PathBuf {
        if let Some(lock) = DATABASE_PATH_OVERRIDE.get()
            && let Some(path) = lock.read().expect("database path lock poisoned").clone()
        {
            return path;
        }

        if let Some(path) = std::env::var_os("MUDU_MOCK_SQLITE_PATH") {
            return PathBuf::from(path);
        }

        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("mudu_mock.db")
    }

    pub async fn query_raw(query_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || {
            let argv = crate::mudu_sys::decode_query_request(&query_in)?;
            let result = Self::sys_query_sync(argv);
            crate::mudu_sys::encode_query_response(&result)
        })
        .await?
    }

    pub async fn command_raw(command_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || {
            let argv = crate::mudu_sys::decode_command_request(&command_in)?;
            let result = Self::sys_command_sync(argv);
            crate::mudu_sys::encode_command_response(&result)
        })
        .await?
    }

    /// `fetch` is not one of the 23 `uni-syscall.wit` syscalls and has no
    /// SyscallPayload v1 route: the cursor bytes pass through untouched.
    pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Ok(query_result)
    }

    /// Validates the MSSP header and routes by message kind: query/command/
    /// batch go to the SQLite emulation, session/KV/relation kinds to the
    /// in-memory KV emulation and the fs kinds to the in-memory fs emulation.
    fn route_frame(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        use crate::mudu_sys::{batch, fs as sys_fs, kv as sys_kv, relation, session};
        let (kind, _) = crate::mudu_sys::payload::decode_any_frame(frame)?;
        match kind {
            crate::mudu_sys::payload::KIND_BATCH => {
                let argv = batch::decode_batch_request(frame)?;
                batch::encode_batch_response(&Self::sys_command_sync(argv))
            }
            crate::mudu_sys::payload::KIND_OPEN => {
                let worker_id = session::decode_open_request(frame)?;
                session::encode_open_response(&super::kv::open(worker_id))
            }
            crate::mudu_sys::payload::KIND_CLOSE => {
                let oid = session::decode_close_request(frame)?;
                session::encode_close_response(&super::kv::close(oid))
            }
            crate::mudu_sys::payload::KIND_GET => {
                let (oid, key) = sys_kv::decode_get_request(frame)?;
                sys_kv::encode_get_response(&super::kv::get(oid, key))
            }
            crate::mudu_sys::payload::KIND_PUT => {
                let (oid, key, value) = sys_kv::decode_put_request(frame)?;
                sys_kv::encode_put_response(&super::kv::put(oid, key, value))
            }
            crate::mudu_sys::payload::KIND_DELETE => {
                let (oid, key) = sys_kv::decode_delete_request(frame)?;
                sys_kv::encode_delete_response(&super::kv::delete(oid, key))
            }
            crate::mudu_sys::payload::KIND_RANGE => {
                let (oid, start, end) = sys_kv::decode_range_request(frame)?;
                sys_kv::encode_range_response(&super::kv::range(oid, start, end))
            }
            crate::mudu_sys::payload::KIND_RELATION_GET => {
                let (oid, table, key, select) = relation::decode_relation_get_request(frame)?;
                relation::encode_relation_get_response(&super::kv::relation_get(
                    oid, table, key, select,
                ))
            }
            crate::mudu_sys::payload::KIND_RELATION_UPDATE => {
                let (oid, table, key, values, deltas) =
                    relation::decode_relation_update_request(frame)?;
                relation::encode_relation_update_response(&super::kv::relation_update(
                    oid, table, key, values, deltas,
                ))
            }
            crate::mudu_sys::payload::KIND_RELATION_INSERT => {
                let (oid, table, key, values) = relation::decode_relation_insert_request(frame)?;
                relation::encode_relation_insert_response(&super::kv::relation_insert(
                    oid, table, key, values,
                ))
            }
            crate::mudu_sys::payload::KIND_FS_OPEN => {
                let argv = sys_fs::decode_fs_open_request(frame)?;
                sys_fs::encode_fs_open_response(&super::fs::fs_open(argv))
            }
            crate::mudu_sys::payload::KIND_FS_CLOSE => {
                let fd = sys_fs::decode_fs_close_request(frame)?;
                sys_fs::encode_fs_close_response(&super::fs::fs_close(fd))
            }
            crate::mudu_sys::payload::KIND_FS_READ => {
                let (fd, len) = sys_fs::decode_fs_read_request(frame)?;
                sys_fs::encode_fs_read_response(&super::fs::fs_read(fd, len))
            }
            crate::mudu_sys::payload::KIND_FS_WRITE => {
                let (fd, data) = sys_fs::decode_fs_write_request(frame)?;
                sys_fs::encode_fs_write_response(&super::fs::fs_write(fd, data))
            }
            crate::mudu_sys::payload::KIND_FS_PREAD => {
                let (fd, offset, len) = sys_fs::decode_fs_pread_request(frame)?;
                sys_fs::encode_fs_pread_response(&super::fs::fs_pread(fd, offset, len))
            }
            crate::mudu_sys::payload::KIND_FS_PWRITE => {
                let (fd, offset, data) = sys_fs::decode_fs_pwrite_request(frame)?;
                sys_fs::encode_fs_pwrite_response(&super::fs::fs_pwrite(fd, offset, data))
            }
            crate::mudu_sys::payload::KIND_FS_LSEEK => {
                let (fd, offset, whence) = sys_fs::decode_fs_lseek_request(frame)?;
                sys_fs::encode_fs_lseek_response(&super::fs::fs_lseek(fd, offset, whence))
            }
            crate::mudu_sys::payload::KIND_FS_FSTAT => {
                let fd = sys_fs::decode_fs_fstat_request(frame)?;
                sys_fs::encode_fs_fstat_response(&super::fs::fs_fstat(fd))
            }
            crate::mudu_sys::payload::KIND_FS_STAT => {
                let (oid, path) = sys_fs::decode_fs_stat_request(frame)?;
                sys_fs::encode_fs_stat_response(&super::fs::fs_stat(oid, path))
            }
            crate::mudu_sys::payload::KIND_FS_FSYNC => {
                let fd = sys_fs::decode_fs_fsync_request(frame)?;
                sys_fs::encode_fs_fsync_response(&super::fs::fs_fsync(fd))
            }
            crate::mudu_sys::payload::KIND_FS_READDIR => {
                let (oid, path) = sys_fs::decode_fs_readdir_request(frame)?;
                sys_fs::encode_fs_readdir_response(&super::fs::fs_readdir(oid, path))
            }
            other => Err(ApiError::Decode(format!(
                "mock does not emulate syscall message kind {other}"
            ))),
        }
    }

    /// Runs `route_frame` on the blocking pool for one raw frame handler.
    async fn routed(frame: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || Self::route_frame(&frame)).await?
    }

    pub async fn batch_raw(batch_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(batch_in).await
    }

    pub async fn open_raw(open_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(open_in).await
    }

    pub async fn close_raw(close_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(close_in).await
    }

    pub async fn get_raw(get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(get_in).await
    }

    pub async fn put_raw(put_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(put_in).await
    }

    pub async fn delete_raw(delete_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(delete_in).await
    }

    pub async fn range_raw(range_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(range_in).await
    }

    pub async fn relation_get_raw(relation_get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_get_in).await
    }

    pub async fn relation_update_raw(relation_update_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_update_in).await
    }

    pub async fn relation_insert_raw(relation_insert_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(relation_insert_in).await
    }

    pub async fn fs_open_raw(fs_open_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_open_in).await
    }

    pub async fn fs_close_raw(fs_close_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_close_in).await
    }

    pub async fn fs_read_raw(fs_read_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_read_in).await
    }

    pub async fn fs_write_raw(fs_write_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_write_in).await
    }

    pub async fn fs_pread_raw(fs_pread_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_pread_in).await
    }

    pub async fn fs_pwrite_raw(fs_pwrite_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_pwrite_in).await
    }

    pub async fn fs_lseek_raw(fs_lseek_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_lseek_in).await
    }

    pub async fn fs_fstat_raw(fs_fstat_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_fstat_in).await
    }

    pub async fn fs_stat_raw(fs_stat_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_stat_in).await
    }

    pub async fn fs_fsync_raw(fs_fsync_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_fsync_in).await
    }

    pub async fn fs_readdir_raw(fs_readdir_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Self::routed(fs_readdir_in).await
    }

    pub async fn sys_command(argv: UniCommandArgv) -> UniCommandReturn {
        tokio::task::spawn_blocking(move || Self::sys_command_sync(argv))
            .await
            .unwrap_or_else(|error| Self::command_error(error.to_string()))
    }

    pub async fn sys_query(argv: UniQueryArgv) -> UniQueryReturn {
        tokio::task::spawn_blocking(move || Self::sys_query_sync(argv))
            .await
            .unwrap_or_else(|error| Self::query_error(error.to_string()))
    }

    fn sys_command_sync(argv: UniCommandArgv) -> UniCommandReturn {
        match Self::try_sys_command(argv) {
            Ok(result) => UniCommandReturn::Ok(result),
            Err(message) => Self::command_error(message),
        }
    }

    fn sys_query_sync(argv: UniQueryArgv) -> UniQueryReturn {
        match Self::try_sys_query(argv) {
            Ok(result) => UniQueryReturn::Ok(result),
            Err(message) => Self::query_error(message),
        }
    }

    fn try_sys_command(argv: UniCommandArgv) -> Result<UniCommandResult, String> {
        let connection = Self::open_connection()?;
        let mut statement = connection
            .prepare(&argv.command.sql_string)
            .map_err(|error| error.to_string())?;
        let params = Self::to_db_values(argv.param_list.params)?;
        let affected_rows = statement
            .execute(params_from_iter(params.iter()))
            .map_err(|error| error.to_string())?;

        Ok(UniCommandResult {
            affected_rows: affected_rows as u64,
        })
    }

    fn try_sys_query(argv: UniQueryArgv) -> Result<UniQueryResult, String> {
        let connection = Self::open_connection()?;
        let mut statement = connection
            .prepare(&argv.query.sql_string)
            .map_err(|error| error.to_string())?;
        let column_count = statement.column_count();
        let column_names = statement
            .column_names()
            .iter()
            .map(|name| (*name).to_string())
            .collect::<Vec<_>>();
        let params = Self::to_db_values(argv.param_list.params)?;
        let mut rows = statement
            .query(params_from_iter(params.iter()))
            .map_err(|error| error.to_string())?;
        let mut row_set = Vec::new();
        let mut inferred_types = vec![None; column_count];

        while let Some(row) = rows.next().map_err(|error| error.to_string())? {
            row_set.push(Self::read_row(row, &mut inferred_types)?);
        }

        Ok(UniQueryResult {
            tuple_desc: Self::build_tuple_desc(column_names, inferred_types),
            result_set: UniResultSet {
                eof: true,
                row_set,
                cursor: Vec::new(),
            },
        })
    }

    fn open_connection() -> Result<Connection, String> {
        let path = Self::database_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }

        Connection::open(path).map_err(|error| error.to_string())
    }

    fn to_db_values(values: Vec<UniDataValue>) -> Result<Vec<Value>, String> {
        values
            .into_iter()
            .map(|value| match value {
                UniDataValue::Scalar(scalar) => Self::to_db_scalar(scalar),
                UniDataValue::Binary(bytes) => Ok(Value::Blob(bytes)),
                other => Err(format!("unsupported sqlite parameter type: {other:?}")),
            })
            .collect()
    }

    fn to_db_scalar(value: UniScalarValue) -> Result<Value, String> {
        match value {
            UniScalarValue::Bool(v) => Ok(Value::Integer(if v { 1 } else { 0 })),
            UniScalarValue::U8(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I8(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U16(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I16(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U32(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I32(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::U64(v) => Ok(Value::Integer(v as i64)),
            UniScalarValue::I64(v) => Ok(Value::Integer(v)),
            UniScalarValue::F32(v) => Ok(Value::Real(v as f64)),
            UniScalarValue::F64(v) => Ok(Value::Real(v)),
            UniScalarValue::Char(v) => Ok(Value::Text(v.to_string())),
            UniScalarValue::String(v) => Ok(Value::Text(v)),
            UniScalarValue::U128(v) | UniScalarValue::I128(v) | UniScalarValue::Blob(v) => {
                Ok(Value::Blob(v))
            }
            UniScalarValue::Numeric(v)
            | UniScalarValue::Date(v)
            | UniScalarValue::Time(v)
            | UniScalarValue::Timestamp(v)
            | UniScalarValue::TimestampTz(v) => Ok(Value::Text(v)),
        }
    }

    fn read_row(
        row: &rusqlite::Row<'_>,
        inferred_types: &mut [Option<UniDataType>],
    ) -> Result<UniTupleRow, String> {
        let mut fields = Vec::with_capacity(row.as_ref().column_count());
        for (index, inferred_type) in inferred_types.iter_mut().enumerate() {
            let value = row.get_ref(index).map_err(|error| error.to_string())?;
            let field = Self::to_uni_data_value(value)?;
            if inferred_type.is_none() {
                *inferred_type = Some(Self::infer_uni_data_type(&field));
            }
            fields.push(field);
        }

        Ok(UniTupleRow { fields })
    }

    fn build_tuple_desc(
        column_names: Vec<String>,
        inferred_types: Vec<Option<UniDataType>>,
    ) -> UniRecordType {
        let record_fields = column_names
            .into_iter()
            .zip(inferred_types)
            .map(|(field_name, field_type)| UniRecordField {
                field_name,
                field_type: field_type.unwrap_or(UniDataType::Scalar(UniScalar::String)),
                field_attrs: Vec::new(),
            })
            .collect();

        UniRecordType {
            record_name: String::new(),
            record_fields,
        }
    }

    fn to_uni_data_value(value: ValueRef<'_>) -> Result<UniDataValue, String> {
        match value {
            ValueRef::Null => Err("NULL value is not supported".to_string()),
            ValueRef::Integer(v) => Ok(UniDataValue::Scalar(UniScalarValue::I64(v))),
            ValueRef::Real(v) => Ok(UniDataValue::Scalar(UniScalarValue::F64(v))),
            ValueRef::Text(v) => Ok(UniDataValue::Scalar(UniScalarValue::String(
                String::from_utf8_lossy(v).into_owned(),
            ))),
            ValueRef::Blob(v) => Ok(UniDataValue::Binary(v.to_vec())),
        }
    }

    fn infer_uni_data_type(value: &UniDataValue) -> UniDataType {
        match value {
            UniDataValue::Scalar(UniScalarValue::Bool(_)) => UniDataType::Scalar(UniScalar::Bool),
            UniDataValue::Scalar(UniScalarValue::U8(_)) => UniDataType::Scalar(UniScalar::U8),
            UniDataValue::Scalar(UniScalarValue::I8(_)) => UniDataType::Scalar(UniScalar::I8),
            UniDataValue::Scalar(UniScalarValue::U16(_)) => UniDataType::Scalar(UniScalar::U16),
            UniDataValue::Scalar(UniScalarValue::I16(_)) => UniDataType::Scalar(UniScalar::I16),
            UniDataValue::Scalar(UniScalarValue::U32(_)) => UniDataType::Scalar(UniScalar::U32),
            UniDataValue::Scalar(UniScalarValue::I32(_)) => UniDataType::Scalar(UniScalar::I32),
            UniDataValue::Scalar(UniScalarValue::U64(_)) => UniDataType::Scalar(UniScalar::U64),
            UniDataValue::Scalar(UniScalarValue::I64(_)) => UniDataType::Scalar(UniScalar::I64),
            UniDataValue::Scalar(UniScalarValue::F32(_)) => UniDataType::Scalar(UniScalar::F32),
            UniDataValue::Scalar(UniScalarValue::F64(_)) => UniDataType::Scalar(UniScalar::F64),
            UniDataValue::Scalar(UniScalarValue::Char(_)) => UniDataType::Scalar(UniScalar::Char),
            UniDataValue::Scalar(UniScalarValue::String(_)) => {
                UniDataType::Scalar(UniScalar::String)
            }
            UniDataValue::Scalar(UniScalarValue::U128(_)) => UniDataType::Scalar(UniScalar::U128),
            UniDataValue::Scalar(UniScalarValue::I128(_)) => UniDataType::Scalar(UniScalar::I128),
            UniDataValue::Scalar(UniScalarValue::Blob(_)) => UniDataType::Scalar(UniScalar::Blob),
            UniDataValue::Scalar(UniScalarValue::Numeric(_)) => {
                UniDataType::Scalar(UniScalar::Numeric)
            }
            UniDataValue::Scalar(UniScalarValue::Date(_)) => UniDataType::Scalar(UniScalar::Date),
            UniDataValue::Scalar(UniScalarValue::Time(_)) => UniDataType::Scalar(UniScalar::Time),
            UniDataValue::Scalar(UniScalarValue::Timestamp(_)) => {
                UniDataType::Scalar(UniScalar::Timestamp)
            }
            UniDataValue::Scalar(UniScalarValue::TimestampTz(_)) => {
                UniDataType::Scalar(UniScalar::TimestampTz)
            }
            UniDataValue::Binary(_) => UniDataType::Scalar(UniScalar::Blob),
            UniDataValue::Array(_) | UniDataValue::Record(_) => {
                UniDataType::Scalar(UniScalar::String)
            }
        }
    }

    fn command_error(message: String) -> UniCommandReturn {
        UniCommandReturn::Err(UniError {
            err_code: 1,
            err_msg: message,
            ..Default::default()
        })
    }

    fn query_error(message: String) -> UniQueryReturn {
        UniQueryReturn::Err(UniError {
            err_code: 1,
            err_msg: message,
            ..Default::default()
        })
    }
}
