use crate::database::db_conn::DBConnSync;
use crate::database::entity::Entity;
use crate::database::entity_set::RecordSet;
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use mudu::common::result::RS;
use std::sync::Arc;

pub struct Context {
    conn: Arc<dyn DBConnSync>,
}

impl Context {
    pub fn new(conn: Arc<dyn DBConnSync>) -> Self {
        Self { conn }
    }

    pub fn db_conn(&self) -> Arc<dyn DBConnSync> {
        self.conn.clone()
    }

    pub fn query<R: Entity>(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<RecordSet<R>> {
        let (rs, ds) = self.conn.query(sql, param)?;
        let r = RecordSet::<R>::new(rs, ds);
        Ok(r)
    }

    pub fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.conn.command(sql, param)
    }
}
