use crate::error::ApiError;
use crate::types::{UniCommandResult, UniCommandReturn, UniQueryReturn};
use crate::{
    UniCommandArgv, UniDatType, UniDatValue, UniError, UniScalar, UniScalarValue,
    UniQueryArgv, UniQueryResult, UniRecordField, UniRecordType, UniResult, UniResultSet,
    UniTupleRow,
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
        if let Some(lock) = DATABASE_PATH_OVERRIDE.get() {
            if let Some(path) = lock.read().expect("database path lock poisoned").clone() {
                return path;
            }
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
            let argv: UniQueryArgv = rmp_serde::from_slice(&query_in)?;
            let result = Self::sys_query_sync(argv);
            Ok(rmp_serde::to_vec(&result)?)
        })
        .await?
    }

    pub async fn command_raw(command_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        tokio::task::spawn_blocking(move || {
            let argv: UniCommandArgv = rmp_serde::from_slice(&command_in)?;
            let result = Self::sys_command_sync(argv);
            Ok(rmp_serde::to_vec(&result)?)
        })
        .await?
    }

    pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
        Ok(query_result)
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
            Ok(result) => UniResult::Ok(result),
            Err(message) => Self::command_error(message),
        }
    }

    fn sys_query_sync(argv: UniQueryArgv) -> UniQueryReturn {
        match Self::try_sys_query(argv) {
            Ok(result) => UniResult::Ok(result),
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

    fn to_db_values(values: Vec<UniDatValue>) -> Result<Vec<Value>, String> {
        values
            .into_iter()
            .map(|value| match value {
                UniDatValue::Scalar(scalar) => Self::to_db_scalar(scalar),
                UniDatValue::Binary(bytes) => Ok(Value::Blob(bytes)),
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
        }
    }

    fn read_row(
        row: &rusqlite::Row<'_>,
        inferred_types: &mut [Option<UniDatType>],
    ) -> Result<UniTupleRow, String> {
        let mut fields = Vec::with_capacity(row.as_ref().column_count());
        for index in 0..row.as_ref().column_count() {
            let value = row.get_ref(index).map_err(|error| error.to_string())?;
            let field = Self::to_uni_dat_value(value)?;
            if inferred_types[index].is_none() {
                inferred_types[index] = Some(Self::infer_uni_dat_type(&field));
            }
            fields.push(field);
        }

        Ok(UniTupleRow { fields })
    }

    fn build_tuple_desc(
        column_names: Vec<String>,
        inferred_types: Vec<Option<UniDatType>>,
    ) -> UniRecordType {
        let record_fields = column_names
            .into_iter()
            .zip(inferred_types)
            .map(|(field_name, field_type)| UniRecordField {
                field_name,
                field_type: field_type.unwrap_or(UniDatType::Scalar(UniScalar::String)),
            })
            .collect();

        UniRecordType {
            record_name: String::new(),
            record_fields,
        }
    }

    fn to_uni_dat_value(value: ValueRef<'_>) -> Result<UniDatValue, String> {
        match value {
            ValueRef::Null => Err("NULL value is not supported".to_string()),
            ValueRef::Integer(v) => Ok(UniDatValue::Scalar(UniScalarValue::I64(v))),
            ValueRef::Real(v) => Ok(UniDatValue::Scalar(UniScalarValue::F64(v))),
            ValueRef::Text(v) => Ok(UniDatValue::Scalar(UniScalarValue::String(
                String::from_utf8_lossy(v).into_owned(),
            ))),
            ValueRef::Blob(v) => Ok(UniDatValue::Binary(v.to_vec())),
        }
    }

    fn infer_uni_dat_type(value: &UniDatValue) -> UniDatType {
        match value {
            UniDatValue::Scalar(UniScalarValue::Bool(_)) => {
                UniDatType::Scalar(UniScalar::Bool)
            }
            UniDatValue::Scalar(UniScalarValue::U8(_)) => {
                UniDatType::Scalar(UniScalar::U8)
            }
            UniDatValue::Scalar(UniScalarValue::I8(_)) => {
                UniDatType::Scalar(UniScalar::I8)
            }
            UniDatValue::Scalar(UniScalarValue::U16(_)) => {
                UniDatType::Scalar(UniScalar::U16)
            }
            UniDatValue::Scalar(UniScalarValue::I16(_)) => {
                UniDatType::Scalar(UniScalar::I16)
            }
            UniDatValue::Scalar(UniScalarValue::U32(_)) => {
                UniDatType::Scalar(UniScalar::U32)
            }
            UniDatValue::Scalar(UniScalarValue::I32(_)) => {
                UniDatType::Scalar(UniScalar::I32)
            }
            UniDatValue::Scalar(UniScalarValue::U64(_)) => {
                UniDatType::Scalar(UniScalar::U64)
            }
            UniDatValue::Scalar(UniScalarValue::I64(_)) => {
                UniDatType::Scalar(UniScalar::I64)
            }
            UniDatValue::Scalar(UniScalarValue::F32(_)) => {
                UniDatType::Scalar(UniScalar::F32)
            }
            UniDatValue::Scalar(UniScalarValue::F64(_)) => {
                UniDatType::Scalar(UniScalar::F64)
            }
            UniDatValue::Scalar(UniScalarValue::Char(_)) => {
                UniDatType::Scalar(UniScalar::Char)
            }
            UniDatValue::Scalar(UniScalarValue::String(_)) => {
                UniDatType::Scalar(UniScalar::String)
            }
            UniDatValue::Binary(_) => UniDatType::Scalar(UniScalar::Blob),
            UniDatValue::Array(_) | UniDatValue::Record(_) => {
                UniDatType::Scalar(UniScalar::String)
            }
        }
    }

    fn command_error(message: String) -> UniCommandReturn {
        UniResult::Err(UniError {
            err_code: 1,
            err_msg: message,
        })
    }

    fn query_error(message: String) -> UniQueryReturn {
        UniResult::Err(UniError {
            err_code: 1,
            err_msg: message,
        })
    }
}
