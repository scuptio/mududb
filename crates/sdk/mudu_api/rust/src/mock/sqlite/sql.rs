//! SQLite execution continuation block: prepares and runs command/query SQL
//! against the emulated database file.

use super::MockSqliteMuduSysCall;
use crate::error::ApiError;
use crate::mudu_sys::batch;
use crate::types::UniCommandResult;
use crate::{UniCommandArgv, UniQueryArgv, UniQueryResult, UniResultSet};
use rusqlite::{Connection, params_from_iter};

impl MockSqliteMuduSysCall {
    /// Routes a `batch` request frame to the SQLite command emulation.
    pub(super) fn route_batch(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let argv = batch::decode_batch_request(frame)?;
        batch::encode_batch_response(&Self::sys_command_sync(argv))
    }

    pub(super) fn try_sys_command(argv: UniCommandArgv) -> Result<UniCommandResult, String> {
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

    pub(super) fn try_sys_query(argv: UniQueryArgv) -> Result<UniQueryResult, String> {
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
}
