
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::db_conn::DBConn;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_stmt::SQLStmt;
use mudu::tuple::datum::DatumDyn;
use mudu::tuple::tuple_item_desc::TupleItemDesc;
use std::sync::Arc;
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

    fn query(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)> {
        self.inner.sync_query(sql, param)
    }

    fn command(&self, sql: &dyn SQLStmt, param: &[&dyn DatumDyn]) -> RS<u64> {
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
    use mudu::this_file;
    use rusqlite::Connection;
    use crate::db_libsql::ls_async_conn::test::__mudu_lib_db_file;
    use crate::db_libsql::ls_conn::create_ls_conn;

    fn test_db_folder() -> String {
        let file = this_file!();
        let path1 = PathBuf::from(file);
        let path2 = path1.parent().unwrap().join("test_db");
        path2.to_str().unwrap().to_string()
    }

    fn execute_sql_file<P:AsRef<Path>>(conn: &Connection, path: P) -> Result<(), Box<dyn std::error::Error>> {
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
                conn.execute(&sql_statement, [])?;

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

    fn prepare_test_db() {
        let folder = test_db_folder();
        let db_path = db_file(&folder);
        let conn = Connection::open(db_path).unwrap();
        let sql_path = sql_file(&folder);
        execute_sql_file(&conn, sql_path).unwrap();
    }
    #[test]
    fn test_ls_conn() {
        prepare_test_db();
        let folder = test_db_folder();
        let conn = create_ls_conn(&folder, &folder).unwrap();

    }
}