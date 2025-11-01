use crate::db_postgres::result_set_pg::ResultSetPG;
use crate::db_postgres::tx_pg::TxPg;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::resolver::sql_resolver::SQLResolver;
use crate::sql_prepare::parse_one::parse_one_query;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::db_conn::DBConn;
use mudu::database::result_set::ResultSet;
use mudu::database::sql_params::SQLParams;
use mudu::database::sql_stmt::SQLStmt;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::tuple::datum_desc::DatumDesc;
use mudu::tuple::tuple_field_desc::TupleFieldDesc;
#[cfg(not(target_arch = "wasm32"))]
use postgres::Client;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_select::StmtSelect;
use sql_parser::ast::stmt_type::{StmtCommand, StmtType};
use std::sync::{Arc, Mutex};

pub fn create_pg_interactive_conn(conn_str: &String, ddl_path: &String) -> RS<Arc<dyn DBConn>> {
    Ok(Arc::new(PGInteractive::new(conn_str, ddl_path)?))
}

struct PGInteractive {
    parser: SQLParser,
    resolver: SQLResolver,
    db_conn: Mutex<(Client, Option<TxPg>)>,
}

impl DBConn for PGInteractive {
    fn exec_sql(&self, _sql_text: &String) -> RS<()> {
        Ok(())
    }

    fn begin_tx(&self) -> RS<XID> {
        let mut conn = self.db_conn.lock().unwrap();
        let transaction = conn.0.transaction().unwrap();
        let xid = uuid::Uuid::new_v4().as_u128() as XID;
        let r = TxPg::new(transaction, xid);
        conn.1 = Some(r);
        Ok(xid)
    }

    fn rollback_tx(&self) -> RS<()> {
        let mut conn = self.db_conn.lock().unwrap();
        if conn.1.is_some() {
            let opt = Option::take(&mut conn.1);
            let tx = opt.unwrap();
            tx.rollback()?;
        }
        Ok(())
    }

    fn commit_tx(&self) -> RS<()> {
        let mut conn = self.db_conn.lock().unwrap();
        if conn.1.is_some() {
            let opt = Option::take(&mut conn.1);
            let tx = opt.unwrap();
            tx.commit()?;
        }
        Ok(())
    }

    fn query(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        self.query_inner(sql, param)
    }

    fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        self.command_inner(sql, param)
    }
}

impl PGInteractive {
    fn new(conn_str: &String, ddl_path: &String) -> RS<PGInteractive> {
        let schema_mgr = Self::build_schema_mgr_from_ddl_sql(ddl_path)?;
        let r = Client::connect(conn_str, postgres::NoTls);
        let client = match r {
            Err(e) => {
                panic!("{:?}", e);
            }
            Ok(c) => c,
        };
        let conn = Self {
            parser: SQLParser::new(),
            resolver: SQLResolver::new(schema_mgr),
            db_conn: Mutex::new((client, None)),
        };
        Ok(conn)
    }

    fn build_schema_mgr_from_ddl_sql(ddl_path: &String) -> RS<SchemaMgr> {
        SchemaMgr::load_from_ddl_path(ddl_path)
    }

    fn query_inner(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
        let sql_string = sql.to_sql_string();
        let stmt = self.parse_one_query(&sql_string)?;
        let resolved = self.resolver.resolve_query(&stmt)?;
        let projection = resolved.projection().clone();
        let row_desc = Arc::new(TupleFieldDesc::new(projection));
        let sql_string = Self::replace_placeholder(&sql_string, resolved.placeholder(), param)?;
        let mut conn = self.db_conn.lock().unwrap();
        let rows = match &mut conn.1 {
            None => conn.0.query(sql_string.as_str(), &[]).unwrap(),
            Some(tx) => {
                let x = tx.transaction();
                x.query(sql_string.as_str(), &[]).unwrap()
            }
        };

        let result_set = ResultSetPG::new(row_desc.clone(), rows);
        Ok((Arc::new(result_set), row_desc))
    }

    fn command_inner(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64> {
        let sql_string = sql.to_sql_string();
        let stmt = self.parse_one_command(&sql_string)?;
        let resolved = self.resolver.resolved_command(&stmt)?;
        let sql = Self::replace_placeholder(&sql_string, resolved.placeholder(), param)?;

        let mut conn = self.db_conn.lock().unwrap();
        let rows = match &mut conn.1 {
            None => conn.0.execute(sql.as_str(), &[]).unwrap(),
            Some(tx) => {
                let x = tx.transaction();
                x.execute(sql_string.as_str(), &[]).unwrap()
            }
        };
        Ok(rows as _)
    }

    fn replace_placeholder(
        sql_string: &String,
        desc: &Vec<DatumDesc>,
        param: &dyn SQLParams,
    ) -> RS<String> {
        let placeholder_str = "?";
        let placeholder_str_len = placeholder_str.len();
        let vec_indices: Vec<_> = sql_string
            .match_indices(placeholder_str)
            .into_iter()
            .collect();
        if desc.len() != param.size() as usize || desc.len() != vec_indices.len() {
            return Err(m_error!(
                EC::ParseErr,
                "parameter and placeholder count mismatch"
            ));
        }

        let mut start_pos = 0;
        let mut sql_after_replaced = "".to_string();
        for i in 0..desc.len() {
            let _s = &sql_string[start_pos..vec_indices[i].0];
            sql_after_replaced.push_str(_s);
            sql_after_replaced.push_str(" ");
            let s = param
                .get_idx_unchecked(i as u64)
                .to_printable(desc[i].dat_type().param())?;
            sql_after_replaced.push_str(s.str());
            sql_after_replaced.push_str(" ");
            start_pos += _s.len() + placeholder_str_len;
        }
        if start_pos != sql_string.len() {
            sql_after_replaced.push_str(&sql_string[start_pos..]);
        }
        sql_after_replaced.push_str(" ");
        Ok(sql_after_replaced)
    }

    fn parse_one_query(&self, sql: &String) -> RS<StmtSelect> {
        parse_one_query(&self.parser, sql)
    }

    fn parse_one_command(&self, sql: &String) -> RS<StmtCommand> {
        let stmt_list = self.parser.parse(sql)?;
        if stmt_list.stmts().len() != 1 {
            return Err(m_error!(
                EC::ParseErr,
                "SQL text must be one select statement"
            ));
        }
        let stmt_command = stmt_list.into_stmts().pop().unwrap();
        match stmt_command {
            StmtType::Command(command) => Ok(command),
            _ => Err(m_error!(EC::ParseErr, "SQL must be command statement")),
        }
    }
}
