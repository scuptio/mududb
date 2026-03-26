use crate::error::ApiError;
use crate::mudu_sys;
use crate::types::{UniCommandResult, UniCommandReturn, UniQueryReturn};
use crate::{UniCommandArgv, UniError, UniQueryArgv, UniQueryResult, UniRecordType, UniResultSet};

pub struct Mudu;

impl Mudu {
    pub async fn command(argv: &UniCommandArgv) -> Result<CommandResponse, ApiError> {
        Ok(CommandResponse::new(mudu_sys::sys_command(argv).await?))
    }

    pub async fn query(argv: &UniQueryArgv) -> Result<QueryResponse, ApiError> {
        Ok(QueryResponse::new(mudu_sys::sys_query(argv).await?))
    }

    pub fn serialize_command(argv: &UniCommandArgv) -> Result<Vec<u8>, ApiError> {
        mudu_sys::serialize_command(argv)
    }

    pub fn serialize_query(argv: &UniQueryArgv) -> Result<Vec<u8>, ApiError> {
        mudu_sys::serialize_query(argv)
    }

    pub fn deserialize_command(bytes: &[u8]) -> Result<UniCommandReturn, ApiError> {
        mudu_sys::deserialize_command_result(bytes)
    }

    pub fn deserialize_query(bytes: &[u8]) -> Result<UniQueryReturn, ApiError> {
        mudu_sys::deserialize_query_result(bytes)
    }
}

#[derive(Debug, Clone)]
pub struct CommandResponse {
    inner: UniCommandReturn,
}

impl CommandResponse {
    pub fn new(inner: UniCommandReturn) -> Self {
        Self { inner }
    }

    pub fn raw(&self) -> &UniCommandReturn {
        &self.inner
    }

    pub fn is_ok(&self) -> bool {
        matches!(self.inner, UniCommandReturn::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self.inner, UniCommandReturn::Err(_))
    }

    pub fn result(&self) -> Option<&UniCommandResult> {
        match &self.inner {
            UniCommandReturn::Ok(result) => Some(result),
            UniCommandReturn::Err(_) => None,
        }
    }

    pub fn error(&self) -> Option<&UniError> {
        match &self.inner {
            UniCommandReturn::Ok(_) => None,
            UniCommandReturn::Err(error) => Some(error),
        }
    }

    pub fn affected_rows(&self) -> Option<u64> {
        self.result().map(|result| result.affected_rows)
    }

    pub fn require_ok(self) -> Result<UniCommandResult, UniError> {
        match self.inner {
            UniCommandReturn::Ok(result) => Ok(result),
            UniCommandReturn::Err(error) => Err(error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct QueryResponse {
    inner: UniQueryReturn,
}

impl QueryResponse {
    pub fn new(inner: UniQueryReturn) -> Self {
        Self { inner }
    }

    pub fn raw(&self) -> &UniQueryReturn {
        &self.inner
    }

    pub fn is_ok(&self) -> bool {
        matches!(self.inner, UniQueryReturn::Ok(_))
    }

    pub fn is_err(&self) -> bool {
        matches!(self.inner, UniQueryReturn::Err(_))
    }

    pub fn result(&self) -> Option<&UniQueryResult> {
        match &self.inner {
            UniQueryReturn::Ok(result) => Some(result),
            UniQueryReturn::Err(_) => None,
        }
    }

    pub fn error(&self) -> Option<&UniError> {
        match &self.inner {
            UniQueryReturn::Ok(_) => None,
            UniQueryReturn::Err(error) => Some(error),
        }
    }

    pub fn tuple_desc(&self) -> Option<&UniRecordType> {
        self.result().map(|result| &result.tuple_desc)
    }

    pub fn result_set(&self) -> Option<&UniResultSet> {
        self.result().map(|result| &result.result_set)
    }

    pub fn require_ok(self) -> Result<UniQueryResult, UniError> {
        match self.inner {
            UniQueryReturn::Ok(result) => Ok(result),
            UniQueryReturn::Err(error) => Err(error),
        }
    }
}
