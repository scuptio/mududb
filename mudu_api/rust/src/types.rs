use crate::universal::uni_error::UniError;
use crate::universal::uni_query_result::UniQueryResult;
use crate::universal::uni_result::UniResult;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniCommandResult {
    pub affected_rows: u64,
}

pub type UniCommandReturn = UniResult<UniCommandResult, UniError>;
pub type UniQueryReturn = UniResult<UniQueryResult, UniError>;
