use crate::universal::uni_oid::UniOid;

use crate::universal::uni_sql_stmt::UniSqlStmt;

use crate::universal::uni_sql_param::UniSqlParam;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniQueryArgv {
    pub oid: UniOid,

    pub query: UniSqlStmt,

    pub param_list: UniSqlParam,
}
