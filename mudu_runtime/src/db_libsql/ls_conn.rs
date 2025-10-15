
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::db_conn::DBConn;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_stmt::SQLStmt;
use mudu::tuple::tuple_field_desc::TupleFieldDesc;
use std::sync::Arc;
use mudu::database::sql_params::SQLParams;
use crate::db_libsql::ls_async_conn::LSSyncConn;
use crate::sql_prepare::sql_prepare::SQLPrepare;

pub fn create_ls_conn(db_path: &String, ddl_path: &String) -> RS<Arc<dyn DBConn>> {
    Ok(Arc::new(LSConn::new(db_path, ddl_path)?))
}

struct LSConn {
    inner: Arc<LSSyncConn>,
}

struct LSConnInner {
    sql_prepare: SQLPrepare,
}

impl LSConn {
    fn new(db_path: &String, ddl_path: &String) -> RS<Self> {
        let inner = LSSyncConn::new(db_path, ddl_path)?;
        Ok(Self {
            inner: Arc::new(inner)
        })
    }
}


impl DBConn for LSConn {
    fn begin_tx(&self) -> RS<XID> {
        self.inner.sync_begin_tx()
    }

    fn rollback_tx(&self) -> RS<()> {
        self.inner.sync_rollback()
    }

    fn commit_tx(&self) -> RS<()> {
        self.inner.sync_commit()
    }

    fn query(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.inner.sync_query(sql, param)
    }

    fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.inner.sync_command(sql, param)
    }
}

unsafe impl Send for LSConn {

}

unsafe impl Sync for LSConn {

}



#[cfg(test)]
mod test {
    use std::fs::File;
    use std::io::{BufRead, BufReader};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::Duration;
    use mudu::this_file;
    use libsql::{params, Connection};
    use tokio::runtime::Builder;
    use tokio::time::sleep;
    use tracing::info;
    use mudu::common::result::RS;
    use mudu::common::xid::XID;
    use mudu::database::db_conn::DBConn;
    use mudu_utils::log::log_setup;
    use mudu_utils::notifier::Notifier;
    use mudu_utils::task::spawn_task;
    use crate::db_libsql::ls_async_conn::test::__mudu_lib_db_file;
    use crate::db_libsql::ls_conn::create_ls_conn;

    fn test_db_folder() -> String {
        let file = this_file!();
        let path1 = PathBuf::from(file);
        let path2 = path1.parent().unwrap().join("test_db");
        path2.to_str().unwrap().to_string()
    }

    async fn execute_sql_file<P:AsRef<Path>>(conn: &Connection, path: P) -> Result<(), Box<dyn std::error::Error>> {
        // open SQL file
        let file = File::open(path)?;
        let reader = BufReader::new(file);

        let mut sql_statement = String::new();

        for line in reader.lines() {
            let line = line?;

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
                conn.execute(&sql_statement, params!([])).await?;

                // prepare for next statement
                sql_statement.clear();
            }
        }

        Ok(())
    }

    fn sql_file(folder:&String) -> String {
        let path1 = PathBuf::from(folder);
        let path2 = path1.join("testdb.ddl.sql");
        path2.to_str().unwrap().to_string()
    }

    fn db_file(folder:&String) -> String {
        __mudu_lib_db_file(folder)
    }

    async fn prepare_test_db() {
        let folder = test_db_folder();
        let db_path = db_file(&folder);
        let db = libsql::Builder::new_local(
            db_path
        ).build().await.unwrap();

        let conn = db.connect().unwrap();
        let sql_path = sql_file(&folder);
        execute_sql_file(&conn, sql_path).await.unwrap();
    }
    #[test]
    fn test_ls_conn() {
        log_setup("info");
        let builder = Builder::new_multi_thread()
            .enable_all()
            .build().unwrap();

        let conn_max = 10;
        builder.block_on(async move {
            let notifier = Notifier::new();
            prepare_test_db().await;
            let folder = test_db_folder();
            let mut join = vec![];
            for i in 0..conn_max {
                let db_path = folder.clone();
                let ddl_path = folder.clone();
                let j = spawn_task(
                    notifier.clone(),
                    &format!("task_{}", i),
                    async move {
                        handle_conn(i, conn_max, db_path, ddl_path).await.unwrap();
                }).unwrap();
                join.push(j);
            }
            for j in join {
                j.await.unwrap();
            }
        });
    }

    async fn handle_conn(i:u32, conn_max:u32, db_path: String, ddl_path: String) -> RS<()> {
        let conn = create_ls_conn(&db_path, &ddl_path)?;
        let tx_max = 2;
        for n in 0..tx_max {
            let xid = conn.begin_tx()?;
            let r = process(i, conn_max, n, tx_max, xid, conn.clone());
            if Ok(()) == r {
                conn.commit_tx()?;
            } else {
                conn.rollback_tx()?;
            }
        }
        Ok(())
    }

    fn process(conn_id:u32, conn_max:u32, n:u32, tx_max:u32,  xid:XID, conn:Arc<dyn DBConn>) -> RS<()> {
        let id = conn_id * tx_max + n;
        let n = conn.command(
            &format!("insert into orders(order_id, user_id, amount, status) VALUES({}, 1, 100, 'status');", id), &[])?;
        let (result, desc) = conn.query(&"select order_id, user_id, amount, status from orders;", &[])?;
        let mut n:u64 = 0;
        while let Some(_row) = result.next()? {
            n +=1 ;
        }
        info!("task {} query {} rows",conn_id, n);
        Ok(())
    }
}