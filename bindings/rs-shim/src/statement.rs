use crate::exports::mududb::component_shim::system;

#[derive(Clone, Default)]
pub struct SqlStmt {
    sql: String,
}

impl SqlStmt {
    pub fn as_string(&self) -> &String {
        &self.sql
    }
}

impl system::GuestSqlStmt for SqlStmt {
    fn new(sql: String) -> Self {
        Self { sql }
    }
}
