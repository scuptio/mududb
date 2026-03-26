use crate::universal::uni_oid::UniOid;

use crate::universal::uni_sql_stmt::UniSqlStmt;

use crate::universal::uni_sql_param::UniSqlParam;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniCommandArgv {
    pub oid: UniOid,

    pub command: UniSqlStmt,

    pub param_list: UniSqlParam,
}

impl Default for UniCommandArgv {
    fn default() -> Self {
        Self {
            oid: Default::default(),

            command: Default::default(),

            param_list: Default::default(),
        }
    }
}
