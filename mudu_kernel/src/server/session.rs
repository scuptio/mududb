use crate::server::parser::Parser;
use std::cell::RefCell;

use async_trait::async_trait;
use futures::Sink;
use pgwire::api::auth::md5pass::hash_md5_password;
use pgwire::api::auth::{AuthSource, LoginInfo, Password};
use pgwire::api::portal::Portal;
use pgwire::api::query::{ExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::{
    DescribePortalResponse, DescribeStatementResponse, QueryResponse, Response, Tag,
};
use pgwire::api::stmt::{QueryParser, StoredStatement};
use pgwire::api::store::PortalStore;
use pgwire::api::{ClientInfo, ClientPortalStore};
use sql_parser::ast::stmt_list::StmtList;

use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::stmt_cmd_run::run_cmd_stmt;
use crate::sql::stmt_query_run::run_query_stmt;
use crate::x_engine::thd_ctx::ThdCtx;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::PgWireBackendMessage;
use sql_parser::ast::stmt_type::StmtType;
use std::fmt::Debug;
use std::sync::Arc;

/// session would be accessed in local thread
pub struct Session {
    xid: RefCell<Option<XID>>,
    ctx: ThdCtx,
    parser: Arc<Parser>,
}

impl Session {
    pub fn new(ctx: ThdCtx) -> Self {
        Self {
            xid: RefCell::new(None),
            ctx,
            parser: Arc::new(Parser::new()),
        }
    }

    fn current_tx(&self) -> Option<XID> {
        todo!()
    }

    fn thd_ctx(&self) -> &ThdCtx {
        &self.ctx
    }
}

pub struct DummyAuthSource;

#[async_trait]
impl AuthSource for DummyAuthSource {
    async fn get_password(&self, info: &LoginInfo) -> PgWireResult<Password> {
        println!("login info: {:?}", info);

        let salt = vec![0, 0, 0, 0];
        let password = "root";

        let hash_password =
            hash_md5_password(info.user().as_ref().unwrap(), password, salt.as_ref());
        Ok(Password::new(Some(salt), hash_password.as_bytes().to_vec()))
    }
}

async fn do_query<'a>(stmt: &StmtType, ctx: &dyn SsnCtx) -> PgWireResult<Response<'a>> {
    let response = match stmt {
        StmtType::Command(stmt) => {
            let rows = run_cmd_stmt(todo!(), ctx)
                .await
                .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
            Response::Execution(Tag::new("OK").with_rows(rows as usize))
        }
        StmtType::Select(stmt) => {
            let (fields, stream) = run_query_stmt(todo!(), ctx)
                .await
                .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
            Response::Query(QueryResponse::new(fields, stream))
        }
    };
    Ok(response)
}

#[async_trait]
impl SimpleQueryHandler for Session {
    async fn do_query<'a, C>(
        &self,
        _client: &mut C,
        query: &'a str,
    ) -> PgWireResult<Vec<Response<'a>>>
    where
        C: ClientInfo + Unpin + Send + Sync,
    {
        let query = self.parser.parse_sql(query, &[]).await?;
        let query_types = query.stmts();
        let mut result = vec![];
        for query_type in query_types {
            let r = do_query(&query_type, self).await?;
            result.push(r);
        }
        Ok(result)
    }
}

#[async_trait]
impl ExtendedQueryHandler for Session {
    type Statement = Arc<StmtList>;
    type QueryParser = Parser;

    fn query_parser(&self) -> Arc<Self::QueryParser> {
        self.parser.clone()
    }

    async fn do_describe_statement<C>(
        &self,
        _client: &mut C,
        target: &StoredStatement<Self::Statement>,
    ) -> PgWireResult<DescribeStatementResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement=Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let root_stmt = target.statement.clone();

        let query_type = todo!();

        match query_type {
            StmtType::Command(_stmt) => Ok(DescribeStatementResponse::new(vec![], vec![])),
            StmtType::Select(q) => Ok(DescribeStatementResponse::new(vec![], vec![])),
        }
    }

    async fn do_describe_portal<C>(
        &self,
        _client: &mut C,
        _target: &Portal<Self::Statement>,
    ) -> PgWireResult<DescribePortalResponse>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement=Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        unimplemented!()
    }

    async fn do_query<'a, 'b: 'a, C>(
        &'b self,
        _client: &mut C,
        portal: &'a Portal<Self::Statement>,
        _max_rows: usize,
    ) -> PgWireResult<Response<'a>>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement=Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let root_stmt = portal.statement.statement.clone();
        let r = do_query(todo!(), self).await?;
        Ok(r)
    }
}

impl SsnCtx for Session {
    fn current_tx(&self) -> Option<XID> {
        self.xid.take()
    }

    fn begin_tx(&self, xid: XID) -> RS<()> {
        let mut x = self.xid.borrow_mut();
        match *x {
            Some(id) => Err(
                m_error!(ER::ExistingSuchElement, format!(
                "existing transaction in current session {}",
                id
            ))),
            None => {
                *x = Some(xid);
                Ok(())
            }
        }
    }

    fn end_tx(&self) -> RS<()> {
        let mut x = self.xid.borrow_mut();
        if let Some(_id) = *x {
            *x = None;
        }
        Ok(())
    }
}

unsafe impl Send for Session {}

unsafe impl Sync for Session {}
