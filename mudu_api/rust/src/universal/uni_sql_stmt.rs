#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniSqlStmt {
    pub sql_string: String,
}
