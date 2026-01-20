use crate::db_turso::turso_conn_inner::TursoConnInner;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::common::xid::XID;
use mudu_contract::database::db_conn::DBConnAsync;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql::DBConn;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, MutexGuard};

pub async fn create_turso_conn(
    db_path: &String,
    app_name: &String,
) -> RS<DBConn> {
    let db_file_path = PathBuf::from(db_path).join(app_name);
    let path = rs_option(db_file_path.to_str(), "path to string error")?.to_string();
    let conn = TursoConn::new(path).await?;
    Ok(DBConn::Async(Arc::new(conn)))
}

pub struct TursoConn {
    turso: Arc<Mutex<TursoConnInner>>
}

impl TursoConn {
    async fn new(db_path: String) -> RS<TursoConn> {
        let conn = TursoConnInner::new(db_path).await?;
        Ok(Self {
            turso: Arc::new(Mutex::new(conn))
        })
    }


    async fn handle_inner<R, F>(&self, f: F) -> RS<R>
    where
        F: AsyncFnOnce(MutexGuard<TursoConnInner>) -> RS<R>,
    {
        let guard = self.turso.lock().await;
        f(guard).await
    }
}


#[async_trait]
impl DBConnAsync for TursoConn {
    async fn prepare(&self, stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
        self.handle_inner(
            async move |inner: MutexGuard<TursoConnInner>| { inner.prepare(stmt).await }
        ).await
    }

    async fn exec_silent(&self, sql_text: String) -> RS<()> {
        self.handle_inner(
            async move |inner: MutexGuard<TursoConnInner>| { inner.exec_silent(sql_text).await }
        ).await
    }

    async fn begin_tx(&self) -> RS<XID> {
        self.handle_inner(
            async |mut inner: MutexGuard<TursoConnInner>| { inner.begin_tx().await }
        ).await
    }

    async fn rollback_tx(&self) -> RS<()> {
        self.handle_inner(
            async |mut inner: MutexGuard<TursoConnInner>| { inner.rollback_tx().await }
        ).await
    }

    async fn commit_tx(&self) -> RS<()> {
        self.handle_inner(
            async |mut inner: MutexGuard<TursoConnInner>| { inner.commit_tx().await }
        ).await
    }

    async fn query(&self,
                   sql: Box<dyn SQLStmt>,
                   param: Box<dyn SQLParams>, ) -> RS<Arc<dyn ResultSetAsync>> {
        let f = async move |inner: MutexGuard<TursoConnInner>| {
            inner.query(sql, param).await
        };
        self.handle_inner(f).await
    }

    async fn execute(&self, sql: Box<dyn SQLStmt>,
                     param: Box<dyn SQLParams>, ) -> RS<u64> {
        let f = async move |inner: MutexGuard<TursoConnInner>| {
            inner.command(sql, param).await
        };
        self.handle_inner(f).await
    }
}