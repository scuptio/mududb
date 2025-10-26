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
use std::mem;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::runtime::Handle;
use tokio::task::block_in_place;

#[derive(Clone)]
pub struct LSSyncConn {
    inner: Arc<LSAsyncConnInner>,
}

struct LSAsyncConnInner {
    conn: Connection,
    prepare: SQLPrepare,
    trans: Mutex<Option<LSTrans>>,
}


fn mudu_lib_db_file<P: AsRef<Path>>(db_path: P) -> RS<String> {
    let path = PathBuf::from(db_path.as_ref());
    let opt = path.to_str();
    match opt {
        Some(t) => { Ok(t.to_string()) }
        None => { Err(m_error!(EC::IOErr, "convert path to string error")) }
    }
}

lazy_static! {
    static ref DB : HashMap<String, Arc<Database>> = HashMap::new();
}


async fn get_db(_path: String) -> RS<Arc<Database>> {
    let db_path = mudu_lib_db_file(_path)?;
    let opt = DB.get_async(&db_path).await;
    let db = match opt {
        Some(db) => {
            return Ok(db.get().clone())
        }
        None => {
            let db = Builder::new_local(&db_path)
                .build().await.map_err(|e| {
                m_error!(EC::DBInternalError, "build libsql DB error", e)
            })?;
            Arc::new(db)
        }
    };

    let db = DB.entry_async(db_path).await
        .or_insert(db).get().clone();
    Ok(db)
}


impl LSSyncConn {
    pub fn new(db_path: &String, app_name: &String, ddl_path: &String) -> RS<Self> {
        let sql_prepare = if !app_name.is_empty() {
            let schema_mgr = SchemaMgr::get_mgr(app_name)
                .ok_or(m_error!(EC::NoneErr, "get schema mgr error"))?;
            SQLPrepare::new_from_schema_mgr(schema_mgr)?
        } else if !ddl_path.is_empty() {
            SQLPrepare::new(&ddl_path)?
        } else {
            return Err(m_error!(EC::NoneErr, "empty DDL"));
        };

        let result = block_in_place(move ||
            Handle::current().block_on(
                async move {
                    let r = LSAsyncConnInner::new(db_path.clone(), sql_prepare).await;
                    r
                }
            )
        );
        let inner = result?;
        Ok(Self {
            inner: Arc::new(inner),
        })
    }

    pub fn sync_begin_tx(&self) -> RS<XID> {
        let inner = self.inner.clone();
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_begin_tx().await
            })
        })
    }

    pub fn sync_query(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let inner = self.inner.clone();
        let sql_boxed = sql.clone_boxed();
        let n = param.size();
        let mut params_boxed = Vec::with_capacity(n as usize);
        for i in 0..n {
            let datum = param.get_idx_unchecked(i);
            let boxed = datum.clone_boxed();
            params_boxed.push(boxed);
        }
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_query(sql_boxed, params_boxed.as_slice()).await
            })
        })
    }

    pub fn sync_command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        let inner = self.inner.clone();
        let sql_boxed = sql.clone_boxed();
        let n = param.size();
        let mut params_boxed = Vec::with_capacity(n as usize);
        for i in 0..n {
            let datum = param.get_idx_unchecked(i);
            let boxed = datum.clone_boxed();
            params_boxed.push(boxed);
        }
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_command(sql_boxed, params_boxed.as_slice()).await
            })
        })
    }

    pub fn sync_commit(&self) -> RS<()> {
        let inner = self.inner.clone();
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_commit().await
            })
        })
    }

    pub fn sync_rollback(&self) -> RS<()> {
        let inner = self.inner.clone();
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_rollback().await
            })
        })
    }
}


impl LSAsyncConnInner {
    pub async fn new(db_path: String, prepare: SQLPrepare) -> RS<Self> {
        let db = get_db(db_path.clone()).await?;
        let conn = db.connect().map_err(|e| {
            m_error!(EC::DBInternalError, "connect libsql DB error", e)
        })?;
        let r1 = conn.execute("PRAGMA busy_timeout = 10000000;", ()).await;
        let r2 = conn.execute("PRAGMA journal_mode = WAL;", ()).await;
        for r in [r1, r2] {
            match r {
                Ok(_) => { Ok(()) }
                Err(e) => {
                    match e {
                        Error::ExecuteReturnedRows => {
                            // We can ignore the error and then the pragma is set
                            // https://github.com/tursodatabase/go-libsql/issues/28#issuecomment-2571633180
                            Ok(())
                        }
                        _ => {
                            Err(m_error!(EC::DBInternalError, "set pragma error", e))
                        }
                    }
                }
            }?;
        }

        Ok(Self {
            conn,
            prepare,
            trans: Default::default(),
        })
    }

    pub async fn async_begin_tx(&self) -> RS<XID> {
        let mut guard = self.trans.lock()
            .map_err(|_e| {
                m_error!(EC::DBInternalError, "lock libsql DB error")
            })?;

        match &mut *guard {
            Some(_tx) => {
                Err(m_error!(EC::DBInternalError, "transaction in processing"))
            }
            None => {
                let trans = self.conn.transaction().await
                    .map_err(|e| {
                        m_error!(EC::DBInternalError, "create transaction libsql DB error", e)
                    })?;
                let tx = LSTrans::new(trans);
                let xid = tx.xid();
                *guard = Some(tx);
                Ok(xid)
            }
        }
    }

    pub fn tx_move_out(&self) -> RS<LSTrans> {
        let mut guard = self.trans.lock()
            .map_err(|_e| {
                m_error!(EC::DBInternalError, "lock libsql DB error")
            })?;
        let mut opt_trans = None;
        mem::swap(&mut *guard, &mut opt_trans);
        match opt_trans {
            Some(tx) => {
                Ok(tx)
            }
            None => {
                Err(m_error!(EC::DBInternalError, "no existing transaction"))
            }
        }
    }

    pub async fn transaction<
        R,
        H: AsyncFn(
            &LSTrans,
            &str,
        ) -> RS<R>>(&self, h: H, sql: &str) -> RS<R> {
        let mut guard = self.trans.lock()
            .map_err(|_e| {
                m_error!(EC::DBInternalError, "lock libsql DB error")
            })?;
        match &mut *guard {
            Some(tx) => {
                let r = h(tx, sql).await?;
                Ok(r)
            }
            None => {
                Err(m_error!(EC::DBInternalError, "no existing transaction"))
            }
        }
    }


    async fn async_query<
        SQL: AsSQLStmtRef,
        PARAMS: AsSlice<Element=Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let (s, desc) = self.prepare.replace_query(sql, param)?;
        let _desc = desc.clone();
        let rs = self.transaction(
            async move |tx, s| {
                tx.query(&s, params!([]), _desc.clone()).await
            }, &s,
        ).await?;
        Ok((rs, desc))
    }

    async fn async_command<
        SQL: AsSQLStmtRef,
        PARAMS: AsSlice<Element=Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<u64> {
        let s = self.prepare.replace_command(sql, param)?;
        let affected_rows = self.transaction(
            async move |tx, s| {
                tx.command(&s, params!([])).await
            }, &s,
        ).await?;
        Ok(affected_rows)
    }

    async fn async_commit(&self) -> RS<()> {
        let tx = self.tx_move_out()?;
        tx.commit().await?;
        Ok(())
    }

    async fn async_rollback(&self) -> RS<()> {
        let tx = self.tx_move_out()?;
        tx.rollback().await?;
        Ok(())
    }
}


#[cfg(test)]
pub mod test {
    use crate::db_libsql::ls_async_conn::mudu_lib_db_file;
    use std::path::Path;

    pub fn __mudu_lib_db_file<P: AsRef<Path>>(folder: P) -> String {
        mudu_lib_db_file(folder).unwrap()
    }
}