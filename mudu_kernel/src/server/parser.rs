use async_trait::async_trait;
use pgwire::api::stmt::QueryParser;
use pgwire::api::Type;
use pgwire::error::{PgWireError, PgWireResult};
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_list::StmtList;
use std::sync::Arc;
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

    async fn parse_sql(&self, sql: &str, _types: &[Type]) -> PgWireResult<Self::Statement> {
        info!("parsing statement: {}", sql);
        let parser = SQLParser::new();
        let r_tree = parser.parse(sql);
        let tree = r_tree.map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        Ok(Arc::new(tree))
    }
}
