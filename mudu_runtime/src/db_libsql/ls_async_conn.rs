use crate::db_libsql::ls_trans::LSTrans;
use libsql::{params, Builder, Connection, Database};
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicPtr, Ordering};
use as_slice::AsSlice;
use lazy_static::lazy_static;
use tokio::runtime::Handle;
use tokio::task::block_in_place;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_stmt::{AsSQLStmtRef, SQLStmt};
use mudu::tuple::datum::{AsDatumDynRef, DatumDyn};
use mudu::tuple::tuple_item_desc::TupleItemDesc;
use crate::sql_prepare::sql_prepare::SQLPrepare;


struct LSConn {
    inner:Arc<LSAsyncConnInner>
}

struct LSAsyncConnInner {
    conn: Connection,
    prepare:SQLPrepare,
    trans: Mutex<Option<Arc<LSTrans>>>,
}


lazy_static! {
    static ref DB : HashMap<String, Arc<Database>> = HashMap::new();
}


async fn get_db(path:String) -> RS<Arc<Database>> {
    let opt =  DB.get_async(&path).await;
    let db = match opt {
        Some(db) => {
            return Ok(db.get().clone())
        },
        None => {
            let db = Builder::new_local(&path)
                .build().await.map_err(|e| {
                m_error!(EC::DBOpenError, "build libsql DB error", e)
            })?;
            Arc::new(db)
        }
    };

    let db = DB.entry_async(path).await.or_insert(db).get().clone();
    Ok(db)
}


impl LSConn {
    pub fn new() -> RS<Self> {
        todo!()
    }

    fn sync_query(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        let inner = self.inner.clone();
        let sql_boxed = sql.clone_boxed();
        let param_boxed = param.iter().map(|e| {
            e.clone_boxed()
        }).collect::<Vec<_>>();
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_query(sql_boxed, param_boxed.as_slice()).await
            })
        })
    }

    fn sync_command(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<u64> {
        let inner = self.inner.clone();
        let sql_boxed = sql.clone_boxed();
        let param_boxed = param.iter().map(|e| {
            e.clone_boxed()
        }).collect::<Vec<_>>();
        block_in_place(move || {
            Handle::current().block_on(async {
                inner.async_command(sql_boxed, param_boxed.as_slice()).await
            })
        })
    }
}

impl LSAsyncConnInner {
    pub async fn new(db_path: String, prepare:SQLPrepare) -> RS<Self> {
        let db = get_db(db_path).await?;
        let conn = db.connect().map_err(|e| {
            m_error!(EC::DBOpenError, "connect libsql DB error", e)
        })?;
        Ok(Self {
            conn,
            prepare,
            trans: Default::default(),
        })
    }

    pub async fn transaction(&self) -> RS<Arc<LSTrans>> {
        let trans = self.conn.transaction().await
            .map_err(|e| {
                m_error!(EC::DBOpenError, "create transaction libsql DB error", e)
        })?;

        let mut guard = self.trans.lock()
            .map_err(|e|{
                m_error!(EC::DBOpenError, "lock libsql DB error")
        })?;
        match &mut *guard {
            Some(tx) => {
                Ok(tx.clone())
            }
            None => {
                let ls_trans = Arc::new(LSTrans::new(self.conn.clone(), trans));
                *guard = Some(ls_trans.clone());
                Ok(ls_trans.clone())
            }
        }
    }


    async fn async_query<
        SQL:AsSQLStmtRef,
        PARAMS: AsSlice<Element = Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        let tx = self.transaction().await?;
        let (s, desc) = self.prepare.replace_query(sql, param)?;
        let rs = tx.query(&s, params!([]), desc.clone()).await?;
        Ok((rs, desc))
    }

    async fn async_command<
        SQL:AsSQLStmtRef,
        PARAMS: AsSlice<Element = Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<u64> {
        let tx = self.transaction().await?;
        let s = self.prepare.replace_command(sql, param)?;
        let affected_rows = tx.command(&s, params!([])).await?;
        Ok(affected_rows)
    }
}