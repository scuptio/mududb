use async_trait::async_trait;
use pgwire::api::stmt::QueryParser;
use pgwire::api::Type;
use pgwire::error::{PgWireError, PgWireResult};
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_list::StmtList;
use std::sync::Arc;
use pgwire::api::portal::Format;
use pgwire::api::results::FieldInfo;
use tracing::info;

pub struct Parser {}

impl Parser {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl QueryParser for Parser {
    type Statement = Arc<StmtList>;

    async fn parse_sql<C>(&self,
                          client: &C,
                          sql: &str,
                          types: &[Option<Type>]) -> PgWireResult<Self::Statement> {
        info!("parsing statement: {}", sql);
        let parser = SQLParser::new();
        let r_tree = parser.parse(sql);
        let tree = r_tree.map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        Ok(Arc::new(tree))
    }

    fn get_parameter_types(&self, _stmt: &Self::Statement) -> PgWireResult<Vec<Type>> {
        todo!()
    }

    fn get_result_schema(&self, _stmt: &Self::Statement, column_format: Option<&Format>) -> PgWireResult<Vec<FieldInfo>> {
        todo!()
    }
}
