use crate::async_utils::blocking;
use crate::db_libsql::ls_desc;
use crate::db_libsql::ls_trans::LSTrans;
use as_slice::AsSlice;
use lazy_static::lazy_static;
use libsql::{params, Builder, Connection, Database, Error};
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::{AsSQLStmtRef, SQLStmt};
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::datum::{AsDatumDynRef, DatumDyn};
use scc::HashMap;
use std::io::{BufRead, BufReader, Cursor};
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LSSyncConn {
    inner: Arc<LSAsyncConnInner>,
}

struct LSAsyncConnInner {
    conn: Connection,
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
    pub fn new(db_path: &String, app_name: &String, _ddl_path: &String) -> RS<Self> {
        let _db_path = db_path.clone();
        let _app_name = app_name.clone();
        let result = blocking::run_async(async move {
            let r = LSAsyncConnInner::new(_db_path, _app_name).await;
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
        let desc = param.param_tuple_desc()?;

        self.inner
            .async_query(sql_boxed, params_boxed.as_slice(), desc.into_fields())
    }

    pub fn sync_command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        let sql_text = sql.to_string();
        let n = param.size();
        let mut params_boxed = Vec::with_capacity(n as usize);
        for i in 0..n {
            let datum = param.get_idx_unchecked(i);
            let boxed = datum.clone_boxed();
            params_boxed.push(boxed);
        }
        let desc = param.param_tuple_desc()?;
        self.inner
            .async_command(sql_text, params_boxed, desc.into_fields())
    }

    pub fn sync_commit(&self) -> RS<()> {
        self.inner.async_commit()
    }

    pub fn sync_rollback(&self) -> RS<()> {
        self.inner.async_rollback()
    }

    pub fn libsql_connection(&self) -> Connection {
        self.inner.conn.clone()
    }
}

impl LSAsyncConnInner {
    pub async fn new(db_path: String, app_name: String) -> RS<Self> {
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
        let ls_trans = opt.ok_or_else(|| m_error!(EC::NoSuchElement, "no existing transaction"))?;
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

    fn async_query<SQL: AsSQLStmtRef, PARAMS: AsSlice<Element=Item>, Item: AsDatumDynRef>(
        &self,
        sql: SQL,
        param: PARAMS,
        desc: Vec<DatumDesc>,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let conn = self.conn.clone();
        let trans = self.trans.clone();
        let param_boxed = param
            .as_slice()
            .iter()
            .map(|e| e.as_datum_dyn_ref().clone_boxed())
            .collect();
        let sql_str = sql.as_sql_stmt_ref().to_string();
        let f = async move { Self::async_query_gut(conn, trans, sql_str, param_boxed, desc).await };
        let r = blocking::run_async(f)?;
        let (rs, desc) = r?;
        Ok((rs, desc))
    }

    async fn replace_query(
        connection: Connection,
        sql: String,
        param: Vec<Box<dyn DatumDyn>>,
        param_desc: Vec<DatumDesc>,
    ) -> RS<(String, Arc<TupleFieldDesc>)> {
        let sql = Self::replace_placeholder(&sql, &param_desc, &param)?;
        let desc = ls_desc::desc_projection(&connection, &sql).await?;
        Ok((sql, Arc::new(TupleFieldDesc::new(desc))))
    }

    async fn async_query_gut(
        conn: Connection,
        trans: LockedTrans,
        sql: String,
        params: Vec<Box<dyn DatumDyn>>,
        param_desc: Vec<DatumDesc>,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let (sql, result_desc) = Self::replace_query(conn, sql, params, param_desc).await?;
        let _desc = result_desc.clone();
        let rs = Self::transaction(
            trans,
            async move |tx, s| tx.query(&s, params!([]), _desc.clone()).await,
            &sql,
        )
            .await?;
        Ok((rs, result_desc))
    }

    pub fn replace_placeholder(
        sql_string: &String,
        desc: &Vec<DatumDesc>,
        param: &Vec<Box<dyn DatumDyn>>,
    ) -> RS<String> {
        let placeholder_str = "?";
        let placeholder_str_len = placeholder_str.len();
        let vec_indices: Vec<_> = sql_string
            .match_indices(placeholder_str)
            .into_iter()
            .collect();
        if desc.len() != param.as_slice().len() || desc.len() != vec_indices.len() {
            return Err(m_error!(
                EC::ParseErr,
                "parameter and placeholder count mismatch"
            ));
        }

        let mut start_pos = 0;
        let mut sql_after_replaced = "".to_string();
        for i in 0..desc.len() {
            let _s = &sql_string[start_pos..vec_indices[i].0];
            sql_after_replaced.push_str(_s);
            sql_after_replaced.push_str(" ");
            let s = param[i].to_textual(desc[i].dat_type())?;
            sql_after_replaced.push_str(s.as_str());
            sql_after_replaced.push_str(" ");
            start_pos += _s.len() + placeholder_str_len;
        }
        if start_pos != sql_string.len() {
            sql_after_replaced.push_str(&sql_string[start_pos..]);
        }
        sql_after_replaced.push_str(" ");
        Ok(sql_after_replaced)
    }

    fn async_command(
        &self,
        sql: String,
        param: Vec<Box<dyn DatumDyn>>,
        desc: Vec<DatumDesc>,
    ) -> RS<u64> {
        let sql = Self::replace_placeholder(&sql, &desc, &param)?;
        let trans = self.trans.clone();
        let result = blocking::run_async(async move { Self::async_command_gut(trans, sql).await })?;
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
