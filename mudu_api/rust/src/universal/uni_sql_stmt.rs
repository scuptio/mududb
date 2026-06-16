#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniSqlStmt {
    pub sql_string: String,
}

