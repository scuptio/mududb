#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniSqlStmt {
    pub sql_string: String,
}

impl Default for UniSqlStmt {
    fn default() -> Self {
        Self {
            sql_string: Default::default(),
        }
    }
}
