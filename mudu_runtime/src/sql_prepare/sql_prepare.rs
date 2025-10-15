use std::sync::Arc;
use as_slice::AsSlice;
use mudu::common::result::RS;
use mudu::database::sql_stmt::{AsSQLStmtRef, SQLStmt};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::tuple::datum::{AsDatumDynRef, DatumDyn};
use mudu::tuple::datum_desc::DatumDesc;
use mudu::tuple::tuple_field_desc::TupleFieldDesc;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_select::StmtSelect;
use sql_parser::ast::stmt_type::StmtCommand;
use crate::resolver::schema_mgr::SchemaMgr;
use crate::resolver::sql_resolver::SQLResolver;
use crate::sql_prepare::parse_one::{parse_one_command, parse_one_query};

pub struct SQLPrepare {
    parser: SQLParser,
    resolver: SQLResolver,
}

impl SQLPrepare {
    pub fn new(ddl_path:&String) -> RS<SQLPrepare> {
        let schema_mgr = SchemaMgr::load_from_ddl_path(ddl_path)?;
        Ok(Self {
            parser: SQLParser::new(),
            resolver: SQLResolver::new(schema_mgr),
        })
    }
    
    fn prepare_sql(&self, _sql:&dyn SQLStmt) -> RS<()> {
        todo!();
        Ok(())
    }
    
    /// FIXME replace_* is a temporary solution
    pub fn replace_query<
        SQL:AsSQLStmtRef,
        PARAMS: AsSlice<Element = Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<(String, Arc<TupleFieldDesc>)> {
        let sql_string = sql.as_sql_stmt_ref().to_sql_string();
        let stmt = self.parse_one_query(&sql_string)?;
        let resolved = self.resolver.resolve_query(&stmt)?;
        let projection = resolved.projection().clone();
        let result_set_desc = Arc::new(TupleFieldDesc::new(projection));
        let sql = Self::replace_placeholder(&sql_string, resolved.placeholder(), param)?;
        Ok((sql, result_set_desc))
    }
    
    /// FIXME replace_* is a temporary solution
    pub fn replace_command<
        SQL:AsSQLStmtRef,
        PARAMS: AsSlice<Element = Item>,
        Item: AsDatumDynRef,
    >(&self, sql: SQL, param: PARAMS) -> RS<String> {
        let sql_string = sql.as_sql_stmt_ref().to_sql_string();
        let stmt = self.parse_one_command(&sql_string)?;
        let resolved = self.resolver.resolved_command(&stmt)?;
        let sql = Self::replace_placeholder(&sql_string, resolved.placeholder(), param)?;
        Ok(sql)
    }
    
    fn parse_one_query(&self, sql:&String) -> RS<StmtSelect> {
        parse_one_query(&self.parser, sql)
    }
    
    fn parse_one_command(&self, sql:&String) -> RS<StmtCommand> {
        parse_one_command(&self.parser, sql)
    }
    
    fn replace_placeholder<
        PARAMS: AsSlice <Element = Item> + ,
        Item: AsDatumDynRef
    >(
        sql_string: &String,
        desc: &Vec<DatumDesc>, param: PARAMS
    ) -> RS<String> {
        let placeholder_str = "?";
        let placeholder_str_len = placeholder_str.len();
        let vec_indices: Vec<_> = sql_string.match_indices(placeholder_str).into_iter().collect();
        if desc.len() != param.as_slice().len() || desc.len() != vec_indices.len() {
            return Err(m_error!(EC::ParseErr, "parameter and placeholder count mismatch"));
        }

        let mut start_pos = 0;
        let mut sql_after_replaced = "".to_string();
        for i in 0..desc.len() {
            let _s = &sql_string[start_pos..vec_indices[i].0];
            sql_after_replaced.push_str(_s);
            sql_after_replaced.push_str(" ");
            let s = param.as_slice()[i].as_datum_dyn_ref().to_printable(desc[i].dat_type().param())?;
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

}