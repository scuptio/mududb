use crate::db_libsql::ls_trans::LSTrans;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::sql_prepare::sql_prepare::SQLPrepare;
use as_slice::AsSlice;
use lazy_static::lazy_static;
use libsql::{params, Builder, Connection, Database, Error};
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_params::SQLParams;
use mudu::database::sql_stmt::{AsSQLStmtRef, SQLStmt};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::tuple::datum::AsDatumDynRef;
use mudu::tuple::tuple_field_desc::TupleFieldDesc;
use scc::HashMap;
use std::io::{BufRead, BufReader, Cursor};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::mem;
use crate::async_utils::blocking;

#[derive(Clone)]
pub struct LSSyncConn {
    inner: Arc<LSAsyncConnInner>,
}

struct LSAsyncConnInner {
    conn: Connection,
    prepare: SQLPrepare,
    trans: LockedTrans,
}

#[derive(Clone)]
struct LockedTrans {
    trans: Arc<Mutex<Option<LSTrans>>>,
}

unsafe impl Send for LockedTrans {}
unsafe impl Sync for LockedTrans {}

impl LockedTrans {
    pub fn tx_set(&self, opt_trans: Option<LSTrans>) -> RS<()> {
        let mut guard = self
            .trans
            .lock()
            .map_err(|_e| m_error!(EC::DBInternalError, "lock libsql DB error"))?;
        let mut opt_trans = opt_trans;
        mem::swap(&mut *guard, &mut opt_trans);
        Ok(())
    }

    pub fn tx_move(&self) -> RS<Option<LSTrans>> {
        let mut guard = self
            .trans
            .lock()
            .map_err(|_e| m_error!(EC::DBInternalError, "lock libsql DB error"))?;
        let mut opt_trans = None;
        mem::swap(&mut *guard, &mut opt_trans);
        Ok(opt_trans)
    }
}

fn mudu_lib_db_file<P: AsRef<Path>>(db_path: P, app_name: String) -> RS<String> {
    let path = PathBuf::from(db_path.as_ref()).join(app_name);
    let opt = path.to_str();
    match opt {
        Some(t) => Ok(t.to_string()),
        None => Err(m_error!(EC::IOErr, "convert path to string error")),
    }
}

lazy_static! {
    static ref DB: HashMap<String, Arc<Database>> = HashMap::new();
}

async fn get_db(path: String, app_name: String) -> RS<Arc<Database>> {
    let db_path = mudu_lib_db_file(path, app_name)?;
    let opt = DB.get_async(&db_path).await;
    let db = match opt {
        Some(db) => return Ok(db.get().clone()),
        None => {
            let db = Builder::new_local(&db_path)
                .build()
                .await
                .map_err(|e| m_error!(EC::DBInternalError, "build libsql DB error", e))?;
            Arc::new(db)
        }
    };

    let db = DB.entry_async(db_path).await.or_insert(db).get().clone();
    Ok(db)
}

impl LSSyncConn {
    pub fn new(db_path: &String, app_name: &String, ddl_path: &String) -> RS<Self> {
        let sql_prepare = if !ddl_path.is_empty() {
            SQLPrepare::new(&ddl_path)?
        } else if !app_name.is_empty() {
            let schema_mgr = SchemaMgr::get_mgr(app_name)
                .ok_or_else(|| m_error!(EC::NoneErr, "get schema mgr error"))?;
            SQLPrepare::new_from_schema_mgr(schema_mgr)?
        } else {
            return Err(m_error!(EC::NoneErr, "empty DDL"));
        };
        let _db_path = db_path.clone();
        let _app_name = app_name.clone();
        let result = blocking::run_async(async move {
            let r = LSAsyncConnInner::new(_db_path, _app_name, sql_prepare).await;
            r
        })?;

        let inner = result?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn exe_sql(&self, text: String) -> RS<()> {
        self.inner.async_run_sql(text)
    }

    pub fn sync_begin_tx(&self) -> RS<XID> {
        let inner = self.inner.clone();
        blocking::run_async(async move { inner.async_begin_tx().await })?
    }

    pub fn sync_query(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let sql_boxed = sql.clone_boxed();
        let n = param.size();
        let mut params_boxed = Vec::with_capacity(n as usize);
        for i in 0..n {
            let datum = param.get_idx_unchecked(i);
            let boxed = datum.clone_boxed();
            params_boxed.push(boxed);
        }

        self.inner.async_query(sql_boxed, params_boxed.as_slice())
    }

    pub fn sync_command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        let sql_boxed = sql.clone_boxed();
        let n = param.size();
        let mut params_boxed = Vec::with_capacity(n as usize);
        for i in 0..n {
            let datum = param.get_idx_unchecked(i);
            let boxed = datum.clone_boxed();
            params_boxed.push(boxed);
        }
        self.inner.async_command(sql_boxed, params_boxed.as_slice())
    }

    pub fn sync_commit(&self) -> RS<()> {
        self.inner.async_commit()
    }

    pub fn sync_rollback(&self) -> RS<()> {
        self.inner.async_rollback()
    }
}

impl LSAsyncConnInner {
    pub async fn new(db_path: String, app_name: String, prepare: SQLPrepare) -> RS<Self> {
        let db = get_db(db_path, app_name).await?;
        let conn = db
            .connect()
            .map_err(|e| m_error!(EC::DBInternalError, "connect libsql DB error", e))?;
        let r1 = conn.execute("PRAGMA busy_timeout = 10000000;", ()).await;
        let r2 = conn.execute("PRAGMA journal_mode = WAL;", ()).await;
        for r in [r1, r2] {
            match r {
                Ok(_) => Ok(()),
                Err(e) => {
                    match e {
                        Error::ExecuteReturnedRows => {
                            // We can ignore the error and then the pragma is set
                            // https://github.com/tursodatabase/go-libsql/issues/28#issuecomment-2571633180
                            Ok(())
                        }
                        _ => Err(m_error!(EC::DBInternalError, "set pragma error", e)),
                    }
                }
            }?;
        }

        Ok(Self {
            conn,
            prepare,
            trans: LockedTrans {
                trans: Arc::new(Default::default()),
            },
        })
    }

    pub async fn async_begin_tx(&self) -> RS<XID> {
        let opt_trans = self.trans.tx_move()?;
        if opt_trans.is_none() {
            let trans = self.conn.transaction().await.map_err(|e| {
                m_error!(EC::DBInternalError, "create transaction libsql DB error", e)
            })?;
            let tx = LSTrans::new(trans);
            let xid = tx.xid();
            self.trans.tx_set(Some(tx))?;
            Ok(xid)
        } else {
            Err(m_error!(EC::ExistingSuchElement, "existing transaction"))
        }
    }

    pub fn tx_move_out(&self) -> RS<LSTrans> {
        let opt = self.trans.tx_move()?;
        let ls_trans = opt.ok_or_else(||m_error!(EC::NoSuchElement, "no existing transaction"))?;
        Ok(ls_trans)
    }

    pub async fn transaction<R, H: AsyncFn(&LSTrans, &str) -> RS<R>>(
        trans: LockedTrans,
        h: H,
        sql: &str,
    ) -> RS<R> {
        let opt_trans = trans
            .tx_move()
            .map_err(|_e| m_error!(EC::DBInternalError, "lock libsql DB error"))?;
        match &opt_trans {
            Some(tx) => {
                let result = h(tx, sql).await;
                trans.tx_set(opt_trans)?;
                let r = result?;
                Ok(r)
            }
            None => Err(m_error!(EC::DBInternalError, "no existing transaction")),
        }
    }

    fn async_query<SQL: AsSQLStmtRef, PARAMS: AsSlice<Element = Item>, Item: AsDatumDynRef>(
        &self,
        sql: SQL,
        param: PARAMS,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let (s, desc) = self.prepare.replace_query(sql, param)?;
        let _desc = desc.clone();
        let trans = self.trans.clone();
        let f = async move { Self::async_query_gut(trans, s, desc).await };
        let r = blocking::run_async(f)?;
        let (rs, desc) = r?;
        Ok((rs, desc))
    }

    async fn async_query_gut(
        trans: LockedTrans,
        sql: String,
        result_desc: Arc<TupleFieldDesc>,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let _desc = result_desc.clone();
        let rs = Self::transaction(
            trans,
            async move |tx, s| tx.query(&s, params!([]), _desc.clone()).await,
            &sql,
        )
        .await?;
        Ok((rs, result_desc))
    }

    fn async_command<SQL: AsSQLStmtRef, PARAMS: AsSlice<Element = Item>, Item: AsDatumDynRef>(
        &self,
        sql: SQL,
        param: PARAMS,
    ) -> RS<u64> {
        let s = self.prepare.replace_command(sql, param)?;
        let trans = self.trans.clone();
        let result = blocking::run_async(async move { Self::async_command_gut(trans, s).await })?;
        result
    }

    async fn async_command_gut(trans: LockedTrans, sql: String) -> RS<u64> {
        let affected_rows = Self::transaction(
            trans,
            async move |tx, s| tx.command(&s, params!([])).await,
            &sql,
        )
        .await?;
        Ok(affected_rows)
    }

    fn async_commit(&self) -> RS<()> {
        let tx = self.tx_move_out()?;
        blocking::run_async(async { tx.commit().await })?
    }

    fn async_rollback(&self) -> RS<()> {
        let tx = self.tx_move_out()?;
        blocking::run_async(async { tx.rollback().await })?
    }

    fn async_run_sql(&self, text: String) -> RS<()> {
        let conn = self.conn.clone();
        blocking::run_async(async { Self::run_sql(conn, text).await })?
    }

    async fn run_sql(conn: Connection, text: String) -> RS<()> {
        // open SQL file

        let cursor = Cursor::new(text);

        let reader = BufReader::new(cursor);

        let mut sql_statement = String::new();

        for line in reader.lines() {
            let line = line.map_err(|e| m_error!(EC::IOErr, "read line error", e))?;

            // ignore commend and empty lines
            let trimmed = line.trim();
            if trimmed.starts_with("--") || trimmed.is_empty() {
                continue;
            }

            // sql statement
            sql_statement.push_str(&line);
            sql_statement.push(' ');

            // if ;, execute this SQL
            if trimmed.ends_with(';') {
                // remove the end ; and empty
                sql_statement = sql_statement.trim().to_string();
                if sql_statement.ends_with(';') {
                    sql_statement.pop();
                }

                // execute SQL statement
                conn.execute(&sql_statement, params!([]))
                    .await
                    .map_err(|e| m_error!(EC::IOErr, "execute sql file error", e))?;

                // prepare for next statement
                sql_statement.clear();
            }
        }

        Ok(())
    }
}
