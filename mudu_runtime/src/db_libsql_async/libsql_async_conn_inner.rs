use crate::db_libsql_async::libsql_desc::desc_projection;
use crate::db_libsql_async::param::LibSQLParam;
use crate::db_libsql_async::result_set::{LibSQLAsyncResultSet, ResultSetLease};
use async_trait::async_trait;
use futures::TryFutureExt;
use lazy_static::lazy_static;
use libsql::{Builder, Connection, Database, Statement, Transaction, params_from_iter};
use mudu::common::result::RS;
use mudu::common::xid::{XID, new_xid};
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::database::db_conn::DBConnSync;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::{Datum, DatumDyn};
use scc::HashMap as SCCHashMap;
use std::collections::HashMap;
use std::error::Error;
use std::sync::{Arc, Mutex as StdMutex};

pub fn create_libsql_conn(
    _db_path: &String,
    _app_name: &String,
    _ddl_path: &String,
) -> RS<Arc<dyn DBConnSync>> {
    todo!()
}

pub struct LibSQLAsyncConnInner {
    conn: Connection,
    trans: Option<Transaction>,
    xid: XID,
    cached_prepared: Arc<StdMutex<HashMap<String, Prepared>>>,
}

lazy_static! {
    static ref TURSO_DB: SCCHashMap<String, Arc<Database>> = SCCHashMap::new();
}

async fn get_db(db_path: &String) -> RS<Arc<Database>> {
    let opt_db = TURSO_DB.get_async(db_path).await;
    match opt_db {
        Some(db) => Ok(db.get().clone()),
        None => {
            let db = Builder::new_local(db_path)
                .build()
                .await
                .map_err(|e| m_error!(EC::IOErr, format!("open database error {}", db_path), e))?;
            let arc_db = Arc::new(db);
            let _ = TURSO_DB.insert_async(db_path.clone(), arc_db.clone()).await;
            Ok(arc_db)
        }
    }
}

impl LibSQLAsyncConnInner {
    pub async fn new(db_path: String) -> RS<Self> {
        let db = get_db(&db_path).await?;
        let connection = db
            .connect()
            .map_err(|e| m_error!(EC::IOErr, format!("connect db error {}", db_path), e))?;
        Ok(Self {
            conn: connection,
            trans: None,
            xid: 0,
            cached_prepared: Arc::new(StdMutex::new(HashMap::new())),
        })
    }

    fn add_prepared(&self, sql: String, prepared: Prepared) {
        let mut guard = self.cached_prepared.lock().unwrap();
        guard.insert(sql, prepared);
    }

    async fn prepared(&self, sql: String, query: bool) -> RS<(String, Prepared)> {
        let opt = {
            let mut guard = self.cached_prepared.lock().unwrap();
            guard.remove(&sql)
        };
        match opt {
            Some(prepared) => Ok((sql, prepared)),
            None => {
                let stmt = self.conn.prepare(&sql).await.map_err(db_error)?;
                let prepared = if query {
                    Prepared::new_query_stmt(sql.clone(), stmt).await?
                } else {
                    Prepared::new_command_stmt(sql.clone(), stmt).await?
                };
                Ok((sql, prepared))
            }
        }
    }
}

pub struct Prepared {
    sql: String,
    stmt: Statement,
    project_tuple_desc: Arc<TupleFieldDesc>,
}

pub struct PreparedStmtImpl {
    prepared: Arc<StdMutex<Option<Prepared>>>,
}

#[async_trait]
impl PreparedStmt for PreparedStmtImpl {
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>> {
        let prepared = self.take_prepared()?;
        let mut lease = PreparedSlotLease {
            slot: self.prepared.clone(),
            prepared: Some(prepared),
        };
        let prepared = lease.prepared.take().unwrap();
        prepared.query_with_lease(params, Box::new(lease)).await
    }

    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64> {
        let mut prepared = self.take_prepared()?;
        let result = prepared.execute(params).await;
        self.restore_prepared(prepared)?;
        result
    }

    async fn desc(&self) -> RS<Arc<TupleFieldDesc>> {
        let guard = self
            .prepared
            .lock()
            .map_err(|_| m_error!(EC::MutexError, "lock prepared stmt error"))?;
        let prepared = guard
            .as_ref()
            .ok_or_else(|| m_error!(EC::ExistingSuchElement, "prepared query is still in use"))?;
        Ok(prepared.project_tuple_desc())
    }

    async fn reset(&self) -> RS<()> {
        let mut guard = self
            .prepared
            .lock()
            .map_err(|_| m_error!(EC::MutexError, "lock prepared stmt error"))?;
        let prepared = guard
            .as_mut()
            .ok_or_else(|| m_error!(EC::ExistingSuchElement, "prepared query is still in use"))?;
        prepared.reset();
        Ok(())
    }
}

impl PreparedStmtImpl {
    fn take_prepared(&self) -> RS<Prepared> {
        self.prepared
            .lock()
            .map_err(|_| m_error!(EC::MutexError, "lock prepared stmt error"))?
            .take()
            .ok_or_else(|| m_error!(EC::ExistingSuchElement, "prepared statement is still in use"))
    }

    fn restore_prepared(&self, prepared: Prepared) -> RS<()> {
        let mut guard = self
            .prepared
            .lock()
            .map_err(|_| m_error!(EC::MutexError, "lock prepared stmt error"))?;
        *guard = Some(prepared);
        Ok(())
    }
}

impl Prepared {
    async fn query_with_lease(
        mut self,
        params: Box<dyn SQLParams>,
        lease: Box<dyn ResultSetLease>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        let libsql_param = to_libsql_params(params.as_ref())?;
        let rows = self
            .stmt
            .query(params_from_iter(libsql_param))
            .await
            .map_err(db_error)?;
        let desc = self.project_tuple_desc.clone();
        Ok(Arc::new(LibSQLAsyncResultSet::new(rows, desc, Some(lease))))
    }

    async fn execute(&mut self, params: Box<dyn SQLParams>) -> RS<u64> {
        let libsql_param = to_libsql_params(params.as_ref())?;
        let rows = self
            .stmt
            .execute(params_from_iter(libsql_param))
            .await
            .map_err(db_error)?;
        self.stmt.reset();
        Ok(rows as u64)
    }

    pub fn project_tuple_desc(&self) -> Arc<TupleFieldDesc> {
        self.project_tuple_desc.clone()
    }

    async fn new_query_stmt(sql: String, stmt: Statement) -> RS<Self> {
        let desc = desc_projection(&stmt).await?;
        Ok(Self {
            sql,
            stmt,
            project_tuple_desc: Arc::new(TupleFieldDesc::new(desc)),
        })
    }

    async fn new_command_stmt(sql: String, stmt: Statement) -> RS<Self> {
        Ok(Self {
            sql,
            stmt,
            project_tuple_desc: Arc::new(TupleFieldDesc::new(Vec::new())),
        })
    }

    fn reset(&mut self) {
        self.stmt.reset();
    }
}

fn db_error<E: Error + 'static>(e: E) -> MError {
    let detail = e.to_string();
    m_error!(EC::IOErr, format!("db error: {}", detail), e)
}

fn to_libsql_params(sql_param: &dyn SQLParams) -> RS<LibSQLParam> {
    let desc = sql_param.param_tuple_desc()?;
    if desc.fields().len() as u64 != sql_param.size() {
        return Err(m_error!(
            EC::DBInternalError,
            "parameter and description mismatch"
        ));
    }
    let n = sql_param.size();
    let mut vec = Vec::with_capacity(n as usize);
    for i in 0..n {
        let datum = sql_param.get_idx_unchecked(i);
        let desc = &desc.fields()[i as usize];
        let value = datum.to_value(desc.dat_type())?;
        let libsql_value = _to_libsql_value(&value, desc.dat_type())?;
        vec.push(libsql_value);
    }
    Ok(LibSQLParam::new(vec))
}

fn _to_libsql_value(datum: &DatValue, ty: &DatType) -> RS<libsql::Value> {
    let id = ty.dat_type_id();
    let v = match id {
        DatTypeID::I32 => libsql::Value::Integer(datum.expect_i32().clone() as _),
        DatTypeID::I64 => libsql::Value::Integer(datum.expect_i64().clone() as _),
        DatTypeID::F32 => libsql::Value::Real(datum.expect_f32().clone() as _),
        DatTypeID::F64 => libsql::Value::Real(datum.expect_f64().clone() as _),
        DatTypeID::String => libsql::Value::Text(datum.expect_string().clone()),
        DatTypeID::Array => libsql::Value::Blob(datum.to_binary(ty)?.into()),
        DatTypeID::Record => libsql::Value::Blob(datum.to_binary(ty)?.into()),
        DatTypeID::Binary => libsql::Value::Blob(datum.to_binary(ty)?.into()),
    };
    Ok(v)
}

impl LibSQLAsyncConnInner {
    pub async fn exec_silent(&self, sql_text: String) -> RS<()> {
        let _ = self.conn.execute_batch(&sql_text).await.map_err(db_error)?;
        Ok(())
    }

    pub async fn begin_tx(&mut self) -> RS<XID> {
        let trans = self.conn.transaction().await.map_err(db_error)?;
        self.trans = Some(trans);
        self.xid = new_xid();
        Ok(self.xid)
    }

    pub fn move_tx(&mut self) -> Option<Transaction> {
        let mut trans = None;
        std::mem::swap(&mut self.trans, &mut trans);
        trans
    }

    pub async fn rollback_tx(&mut self) -> RS<()> {
        let opt_trans = self.move_tx();
        match opt_trans {
            Some(trans) => trans.rollback().await.map_err(db_error),
            None => Ok(()),
        }
    }

    pub async fn commit_tx(&mut self) -> RS<()> {
        let opt_trans = self.move_tx();
        match opt_trans {
            Some(trans) => trans.commit().await.map_err(db_error),
            None => Ok(()),
        }
    }

    pub async fn prepare(&self, sql_stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
        let sql_str = sql_stmt.to_string();
        let (_, prepared) = self.prepared(sql_str, true).await?;
        Ok(Arc::new(PreparedStmtImpl {
            prepared: Arc::new(StdMutex::new(Some(prepared))),
        }))
    }

    pub async fn query(
        &self,
        sql_stmt: Box<dyn SQLStmt>,
        sql_params: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        let sql_str = sql_stmt.to_string();
        let (sql, prepared) = self.prepared(sql_str, true).await?;
        let mut lease = CachedPreparedLease {
            sql,
            cache: self.cached_prepared.clone(),
            prepared: Some(prepared),
        };
        let prepared = lease.prepared.take().unwrap();
        prepared.query_with_lease(sql_params, Box::new(lease)).await
    }

    pub async fn command(
        &self,
        sql_stmt: Box<dyn SQLStmt>,
        sql_params: Box<dyn SQLParams>,
    ) -> RS<u64> {
        let sql = sql_stmt.to_string();
        let (sql, mut prepared) = self.prepared(sql, false).await?;
        let result = prepared.execute(sql_params).await;
        self.add_prepared(sql, prepared);
        result
    }

    pub async fn batch(
        &self,
        sql_stmt: Box<dyn SQLStmt>,
        sql_params: Box<dyn SQLParams>,
    ) -> RS<u64> {
        if sql_params.size() != 0 {
            return Err(m_error!(
                EC::NotImplemented,
                "batch syscall does not support SQL parameters"
            ));
        }
        let sql = sql_stmt.to_string();
        if let Some(trans) = self.trans.as_ref() {
            let before = trans.total_changes();
            let _ = trans.execute_batch(&sql).await.map_err(db_error)?;
            return Ok(trans.total_changes().saturating_sub(before));
        }
        let before = self.conn.total_changes();
        let _ = self.conn.execute_batch(&sql).await.map_err(db_error)?;
        Ok(self.conn.total_changes().saturating_sub(before))
    }
}

struct CachedPreparedLease {
    sql: String,
    cache: Arc<StdMutex<HashMap<String, Prepared>>>,
    prepared: Option<Prepared>,
}

impl ResultSetLease for CachedPreparedLease {
    fn release(mut self: Box<Self>) {
        if let Some(mut prepared) = self.prepared.take() {
            prepared.reset();
            let mut guard = self.cache.lock().unwrap();
            guard.insert(self.sql.clone(), prepared);
        }
    }
}

struct PreparedSlotLease {
    slot: Arc<StdMutex<Option<Prepared>>>,
    prepared: Option<Prepared>,
}

impl ResultSetLease for PreparedSlotLease {
    fn release(mut self: Box<Self>) {
        if let Some(mut prepared) = self.prepared.take() {
            prepared.reset();
            if let Ok(mut guard) = self.slot.lock() {
                *guard = Some(prepared);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use libsql::{Builder, Value, params};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(label: &str) -> String {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir()
            .join(format!("mudu-runtime-{label}-{nanos}.db"))
            .to_str()
            .unwrap()
            .to_string()
    }

    #[tokio::test]
    async fn reset_statement_before_consuming_rows_turns_values_null() {
        let db_path = temp_db_path("reset-before-read");
        let db = Builder::new_local(&db_path).build().await.unwrap();
        let conn = db.connect().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE wallets (
                user_id INT PRIMARY KEY,
                balance INT,
                updated_at INT
            );
            INSERT INTO wallets (user_id, balance, updated_at) VALUES (1, 100, 0);
            "#,
        )
        .await
        .unwrap();

        let mut stmt = conn
            .prepare("SELECT user_id, balance, updated_at FROM wallets WHERE user_id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(params!(1)).await.unwrap();
        stmt.reset();

        let row = rows.next().await.unwrap().unwrap();
        assert!(matches!(row.get_value(0).unwrap(), Value::Null));
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn consuming_rows_before_reset_keeps_values() {
        let db_path = temp_db_path("read-before-reset");
        let db = Builder::new_local(&db_path).build().await.unwrap();
        let conn = db.connect().unwrap();

        conn.execute_batch(
            r#"
            CREATE TABLE wallets (
                user_id INT PRIMARY KEY,
                balance INT,
                updated_at INT
            );
            INSERT INTO wallets (user_id, balance, updated_at) VALUES (1, 100, 0);
            "#,
        )
        .await
        .unwrap();

        let mut stmt = conn
            .prepare("SELECT user_id, balance, updated_at FROM wallets WHERE user_id = ?")
            .await
            .unwrap();
        let mut rows = stmt.query(params!(1)).await.unwrap();

        let row = rows.next().await.unwrap().unwrap();
        assert!(matches!(row.get_value(0).unwrap(), Value::Integer(1)));
        stmt.reset();
        let _ = std::fs::remove_file(db_path);
    }
}
