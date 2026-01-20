use mudu::common::result::RS;
use mudu::common::xid::{new_xid, XID};
use mudu_contract::database::db_conn::DBConnSync;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

use crate::db_turso::param::TursoParam;
use crate::db_turso::result_set::TursoResultSet;
use crate::db_turso::turso_desc::desc_projection;
use async_trait::async_trait;
use futures::TryFutureExt;
use lazy_static::lazy_static;
use mudu::error::ec::EC;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::{Datum, DatumDyn};
use scc::HashMap as SCCHashMap;
use std::error::Error;
use std::sync::Arc;
use tokio::sync::Mutex;
use turso::{params_from_iter, transaction::Transaction, Builder, Connection, Database, Statement};

pub fn create_turso_conn(
    db_path: &String,
    app_name: &String,
    ddl_path: &String,
) -> RS<Arc<dyn DBConnSync>> {
    todo!()
}


pub struct TursoConnInner {
    conn: Connection,
    // Transaction<'conn> ref to &self.conn, cannot use self.trans outside this struct
    trans: Option<Transaction<'static>>,
    xid: XID,
    cached_prepared: SCCHashMap<String, Prepared>,
}

lazy_static! {
    static ref TURSO_DB: SCCHashMap<String, Database> = SCCHashMap::new();
}

fn to_static_unsafe<'conn>(s: Transaction<'conn>) -> Transaction<'static> {
    unsafe {
        std::mem::transmute::<Transaction<'conn>, Transaction<'static>>(s)
    }
}

async fn get_db(db_path: &String) -> RS<Database> {
    let opt_db = TURSO_DB.get_async(db_path).await;
    match opt_db {
        Some(db) => {
            Ok(db.clone())
        }
        None => {
            let db = Builder::new_local(db_path).build().await
                .map_err(|e| {
                    m_error!(EC::IOErr, format!("open database error {}", db_path), e)
                })?;
            let _ = TURSO_DB.insert_async(db_path.clone(), db.clone()).await;
            Ok(db)
        }
    }
}

impl TursoConnInner {
    pub async fn new(db_path: String) -> RS<Self> {
        let db = get_db(&db_path).await?;
        let connection = db.connect().map_err(|e| {
            m_error!(EC::IOErr, format!("connect db error {}", db_path), e)
        })?;
        Ok(Self { conn: connection, trans: None, xid: 0, cached_prepared: Default::default() })
    }

    async fn add_prepared(&self, sql: String, prepared: Prepared) {
        let _ = self.cached_prepared.insert_async(sql, prepared).await;
    }

    async fn prepared(&self, sql: String, query: bool) -> RS<(String, Prepared)> {
        let opt = self.cached_prepared.remove_async(&sql).await;
        match opt {
            Some(e) => {
                Ok(e)
            }
            None => {
                let stmt = self.conn.prepare(&sql).await
                    .map_err(db_error)?;
                let prepared = if query {
                    Prepared::new_query_stmt(stmt).await?
                } else {
                    Prepared::new_command_stmt(stmt).await?
                };
                Ok((sql, prepared))
            }
        }
    }
}


pub struct Prepared {
    stmt: Statement,
    project_tuple_desc: Arc<TupleFieldDesc>,
}

pub struct PreparedStmtImpl {
    prepared: Arc<Mutex<Prepared>>,
}

#[async_trait]
impl PreparedStmt for PreparedStmtImpl {
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>> {
        self.prepared.lock().await.query(params).await
    }

    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64> {
        self.prepared.lock().await.execute(params).await
    }

    async fn desc(&self) -> RS<Arc<TupleFieldDesc>> {
        todo!()
    }

    async fn reset(&self) -> RS<()> {
        todo!()
    }
}

impl Prepared {
    async fn query(&mut self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>> {
        let turso_param = to_turso_params(params.as_ref())?;
        let rows = self.stmt.query(params_from_iter(turso_param)).await.map_err(db_error)?;
        let desc = self.project_tuple_desc.clone();
        self.stmt.reset();
        Ok(Arc::new(TursoResultSet::new(rows, desc)))
    }

    async fn execute(&mut self, params: Box<dyn SQLParams>) -> RS<u64> {
        let turso_param = to_turso_params(params.as_ref())?;
        let rows = self.stmt.execute(params_from_iter(turso_param)).await.map_err(db_error)?;
        self.stmt.reset();
        Ok(rows)
    }

    pub fn project_tuple_desc(&self) -> Arc<TupleFieldDesc> {
        self.project_tuple_desc.clone()
    }

    pub fn statement_mut(&mut self) -> &mut Statement {
        &mut self.stmt
    }

    async fn new_query_stmt(stmt: Statement) -> RS<Self> {
        let desc = desc_projection(&stmt).await?;
        Ok(Self {
            stmt,
            project_tuple_desc: Arc::new(TupleFieldDesc::new(desc)),
        })
    }

    async fn new_command_stmt(stmt: Statement) -> RS<Self> {
        Ok(Self {
            stmt,
            project_tuple_desc: Arc::new(TupleFieldDesc::new(Vec::new())),
        })
    }

    fn reset(&mut self) {
        self.stmt.reset();
    }
}

fn db_error<E: Error + 'static>(e: E) -> MError {
    m_error!(EC::IOErr, "db error", e)
}

fn to_turso_params(sql_param: &dyn SQLParams) -> RS<TursoParam> {
    let desc = sql_param.param_tuple_desc()?;
    if desc.fields().len() as u64 != sql_param.size() {
        return Err(m_error!(EC::DBInternalError, "parameter and description mismatch"))
    }
    let n = sql_param.size();
    let mut vec = Vec::with_capacity(n as usize);
    for i in 0..n {
        let datum = sql_param.get_idx_unchecked(i);
        let desc = &desc.fields()[i as usize];
        let value = datum.to_value(desc.dat_type())?;
        let turso_value = _to_turso_value(&value, desc.dat_type())?;
        vec.push(turso_value);
    }
    Ok(TursoParam::new(vec))
}
fn _to_turso_value(datum: &DatValue, ty: &DatType) -> RS<turso::Value> {
    let id = ty.dat_type_id();
    let v = match id {
        DatTypeID::I32 => {
            turso::Value::Integer(datum.expect_i32().clone() as _)
        }
        DatTypeID::I64 => {
            turso::Value::Integer(datum.expect_i64().clone() as _)
        }
        DatTypeID::F32 => {
            turso::Value::Real(datum.expect_f32().clone() as _)
        }
        DatTypeID::F64 => {
            turso::Value::Real(datum.expect_f64().clone() as _)
        }
        DatTypeID::String => {
            turso::Value::Text(datum.expect_string().clone())
        }
        DatTypeID::Array => {
            turso::Value::Blob(datum.to_binary(ty)?.into())
        }
        DatTypeID::Record => {
            turso::Value::Blob(datum.to_binary(ty)?.into())
        }
        DatTypeID::Binary => {
            turso::Value::Blob(datum.to_binary(ty)?.into())
        }
    };
    Ok(v)
}
impl TursoConnInner {
    pub async fn exec_silent(&self, sql_text: String) -> RS<()> {
        let _ = self.conn.execute(&sql_text, ()).await.map_err(db_error)?;
        Ok(())
    }

    pub async fn begin_tx(&mut self) -> RS<XID> {
        let trans = self.conn.transaction().await.map_err(db_error)?;
        self.trans = Some(to_static_unsafe(trans));
        self.xid = new_xid();
        Ok(self.xid)
    }

    pub fn move_tx<'conn>(&mut self) -> Option<Transaction<'conn>> {
        let mut trans = None;
        std::mem::swap(&mut self.trans, &mut trans);
        trans
    }

    pub async fn rollback_tx(&mut self) -> RS<()> {
        let opt_trans = self.move_tx();
        match opt_trans {
            Some(trans) => {
                trans.rollback().await.map_err(db_error)
            }
            None => { Ok(()) }
        }
    }

    pub async fn commit_tx(&mut self) -> RS<()> {
        let opt_trans = self.move_tx();
        match opt_trans {
            Some(trans) => {
                trans.commit().await.map_err(db_error)
            }
            None => { Ok(()) }
        }
    }
    pub async fn prepare(&self, sql_stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
        let sql_str = sql_stmt.to_string();
        let (_, prepared) = self.prepared(sql_str, true).await?;
        Ok(Arc::new(PreparedStmtImpl {
            prepared: Arc::new(Mutex::new(prepared)),
        }))
    }

    pub async fn query(&self,
                       sql_stmt: Box<dyn SQLStmt>,
                       sql_params: Box<dyn SQLParams>,
    ) -> RS<(Arc<dyn ResultSetAsync>)> {
        let sql_str = sql_stmt.to_string();
        let (sql_str, mut prepared) = self.prepared(sql_str, true).await?;
        let result = prepared.query(sql_params).await;
        self.add_prepared(sql_str, prepared).await;
        result
    }

    pub async fn command(&self, sql_stmt: Box<dyn SQLStmt>, sql_params: Box<dyn SQLParams>) -> RS<u64> {
        let sql = sql_stmt.to_string();
        let (sql, mut prepared) = self.prepared(sql, false).await?;
        let result = prepared.execute(sql_params).await;
        self.add_prepared(sql, prepared).await;
        result
    }
}




