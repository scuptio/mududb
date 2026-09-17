//! Canonical guest facade `mududb.db`: the database session handle.
//!
//! Mirrors the cross-language canonical surface
//! (`doc/dev/binding_api_surface.md`): `open`/`close`/`query`/`command`/
//! `batch` with the AssemblyScript `Database` semantics. Statements and
//! parameters are accepted as `&dyn SQLStmt` / `&dyn SQLParams`, so the
//! existing `sql_stmt!` / `sql_params!` values (string literals, tuples,
//! `SQLParamValue`) work unchanged.

use crate::result::ResultSet;
use crate::sys_interface::sync_api;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// A database session handle; obtain one with [`Database::open`] or
/// [`Database::open_uri`], and release it with [`Database::close`].
pub struct Database {
    id: OID,
}

impl Database {
    /// Open a session on the default worker.
    pub fn open() -> RS<Database> {
        Ok(Database {
            id: sync_api::mudu_open()?,
        })
    }

    /// Open a session on a specific worker. `uri` is either empty (the
    /// default worker, same as [`Database::open`]) or a decimal u128 worker
    /// object id — the same rule as the AssemblyScript `Database.open`.
    pub fn open_uri(uri: &str) -> RS<Database> {
        if uri.is_empty() {
            return Self::open();
        }
        let worker_id: u128 = uri.parse().map_err(|_| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "open uri must be empty or a numeric worker object id: {}",
                    uri
                )
            )
        })?;
        Ok(Database {
            id: sync_api::mudu_open_argv(&UniSessionOpenArgv {
                worker_id: UniOid::from(worker_id),
            })?,
        })
    }

    /// The session object id of this handle.
    pub fn id(&self) -> OID {
        self.id
    }

    /// Close the session, consuming the handle.
    pub fn close(self) -> RS<()> {
        sync_api::mudu_close(self.id)
    }

    /// Run a `SELECT` statement and return the full result set.
    pub fn query(&self, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<ResultSet> {
        // `i32` is a phantom entity: only the raw rows and the column
        // description are consumed; no entity record is ever materialized.
        let entity_set = sync_api::mudu_query::<i32>(self.id, sql, params)?;
        let (raw, desc) = entity_set.into_parts();
        let mut rows = Vec::new();
        while let Some(row) = raw.next()? {
            rows.push(row);
        }
        Ok(ResultSet::from_parts(desc, rows))
    }

    /// Run an `INSERT` / `UPDATE` / `DELETE` statement and return the number
    /// of affected rows.
    pub fn command(&self, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
        sync_api::mudu_command(self.id, sql, params)
    }

    /// Run a batch of statements through the batch syscall path; same
    /// argument and return shape as [`Database::command`].
    pub fn batch(&self, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
        sync_api::mudu_batch(self.id, sql, params)
    }
}
