use std::cell::RefCell;

use async_trait::async_trait;
use futures::{stream, Sink, Stream};
use pgwire::api::auth::md5pass::hash_md5_password;
use pgwire::api::auth::{AuthSource, LoginInfo, Password};
use pgwire::api::portal::{Format, Portal};
use pgwire::api::query::{ExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::results::{DataRowEncoder, DescribePortalResponse, DescribeStatementResponse, FieldInfo, QueryResponse, Response, Tag};
use pgwire::api::stmt::{NoopQueryParser, StoredStatement};
use pgwire::api::store::PortalStore;
use pgwire::api::{ClientInfo, ClientPortalStore, Type, METADATA_DATABASE};


use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use pgwire::error::{ErrorInfo, PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use pgwire::messages::PgWireBackendMessage;

use crate::backend::session_ctx::SessionCtx;
use libsql::params::Params;
use libsql::{Rows, Statement, Value};
use std::fmt::{Debug, Formatter};
use std::sync::Arc;
use tracing::info;

/// session would be accessed in local thread
pub struct Session {
    _xid: RefCell<Option<XID>>,
    ctx: SessionCtx,
    parser: Arc<NoopQueryParser>,
}

impl Session {
    pub fn new(ctx: SessionCtx) -> Self {
        Self {
            _xid: RefCell::new(None),
            ctx,
            parser: Arc::new(NoopQueryParser::new()),
        }
    }
}

pub struct DummyAuthSource {
    context:SessionCtx,
}

impl DummyAuthSource {
    pub fn new(context:SessionCtx) -> Self {
        Self { context }
    }
}

impl Debug for DummyAuthSource {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}

#[async_trait]
impl AuthSource for DummyAuthSource {
    async fn get_password(&self, info: &LoginInfo) -> PgWireResult<Password> {
        info!("login info: {:?}", info);
        let salt = vec![0, 0, 0, 0];
        let password = "root";
        let user = info.user().as_ref().map_or_else(|| {
            Err(PgWireError::ApiError(Box::new(m_error!(EC::InternalErr))))
        }, |e| {
            Ok(e.to_string())
        })?;
        let hash_password =
            hash_md5_password(&user, password, salt.as_ref());
        let db = info.database()
            .map_or_else(|| {
                Err(PgWireError::ApiError(Box::new(m_error!(EC::InternalErr))))
            }, |e| {
                Ok(e.to_string())
            })?;
        self.context.open(&db).await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        Ok(Password::new(Some(salt), hash_password.as_bytes().to_vec()))
    }
}


#[async_trait]
impl SimpleQueryHandler for Session {
    async fn do_query<C>(
        &self,
        _client: &mut C,
        query: &str) -> PgWireResult<Vec<Response>>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let conn = self.ctx.connection().await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        let stmt = conn
            .prepare(query).await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        if query.to_uppercase().starts_with("SELECT") {
            let header = Arc::new(row_desc_from_stmt(&stmt, &Format::UnifiedText)?);
            let rows = stmt.query(()).await
                .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
            let s = encode_row_data(rows, header.clone());
            Ok(vec![Response::Query(QueryResponse::new(header, s))])
        } else {
            conn.execute(query, ()).await
                .map(|affected_rows| {
                    vec![Response::Execution(Tag::new("OK").with_rows(affected_rows as usize))]
                })
                .map_err(|e| PgWireError::ApiError(Box::new(e)))
        }
    }
}

#[async_trait]
impl ExtendedQueryHandler for Session {
    type Statement = String;
    type QueryParser = NoopQueryParser;

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
        let conn = self.ctx.connection().await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        let param_types = target.parameter_types.iter()
            .map(|e|{
                e.as_ref().unwrap().clone()
            }).collect::<Vec<_>>();
        let stmt = conn
            .prepare(&target.statement).await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;

        row_desc_from_stmt(&stmt, &Format::UnifiedBinary)
            .map(|fields| DescribeStatementResponse::new(param_types, fields))
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
        let conn = self.ctx.connection().await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        let stmt = conn
            .prepare(&_target.statement.statement).await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        row_desc_from_stmt(&stmt, &_target.result_column_format).map(DescribePortalResponse::new)
    }

    async fn do_query<C>(
        &self,
        _client: &mut C,
        portal: &Portal<Self::Statement>,
        _max_rows: usize,
    ) -> PgWireResult<Response>
    where
        C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
        C::PortalStore: PortalStore<Statement=Self::Statement>,
        C::Error: Debug,
        PgWireError: From<<C as Sink<PgWireBackendMessage>>::Error>,
    {
        let conn = self.ctx.connection().await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        let query = &portal.statement.statement;
        let stmt = conn.prepare(query).await
            .map_err(|e| PgWireError::ApiError(Box::new(e)))?;
        let params = get_params(portal);
        if query.to_uppercase().starts_with("SELECT") {
            let header = Arc::new(row_desc_from_stmt(&stmt, &portal.result_column_format)?);
            let rows = stmt.query(Params::Positional(params)).await
                .map_err(|e| PgWireError::ApiError(Box::new(e)))?;

            let s = encode_row_data(rows, header.clone());
            Ok(Response::Query(QueryResponse::new(header, s)))
        } else {
            stmt.execute(Params::Positional(params)).await
                .map(|affected_rows| Response::Execution(Tag::new("OK").with_rows(affected_rows)))
                .map_err(|e| PgWireError::ApiError(Box::new(e)))
        }
    }
}

unsafe impl Send for Session {}

unsafe impl Sync for Session {}


fn get_params(portal: &Portal<String>) -> Vec<Value> {
    let mut results = Vec::with_capacity(portal.parameter_len());
    for i in 0..portal.parameter_len() {
        let param_type = portal.statement.parameter_types.get(i)
            .unwrap().as_ref().unwrap().clone();
        // we only support a small amount of types for demo
        match &param_type {
            &Type::BOOL => {
                let param = portal.parameter::<bool>(i, &param_type).unwrap();
                results.push(Value::Integer(param.unwrap() as i64));
            }
            &Type::INT2 => {
                let param = portal.parameter::<i16>(i, &param_type).unwrap();
                results.push(Value::Integer(param.unwrap() as i64));
            }
            &Type::INT4 => {
                let param = portal.parameter::<i32>(i, &param_type).unwrap();
                results.push(Value::Integer(param.unwrap() as i64));
            }
            &Type::INT8 => {
                let param = portal.parameter::<i64>(i, &param_type).unwrap();
                results.push(Value::Integer(param.unwrap()));
            }
            &Type::TEXT | &Type::VARCHAR => {
                let param = portal.parameter::<String>(i, &param_type).unwrap();
                results.push(Value::Text(param.unwrap()));
            }
            &Type::FLOAT4 => {
                let param = portal.parameter::<f32>(i, &param_type).unwrap();
                results.push(Value::Real(param.unwrap() as f64));
            }
            &Type::FLOAT8 => {
                let param = portal.parameter::<f64>(i, &param_type).unwrap();
                results.push(Value::Real(param.unwrap()));
            }
            _ => {
                unimplemented!("parameter type not supported")
            }
        }
    }

    results
}


fn row_desc_from_stmt(stmt: &Statement, format: &Format) -> PgWireResult<Vec<FieldInfo>> {
    stmt.columns()
        .iter()
        .enumerate()
        .map(|(idx, col)| {
            let field_type = col
                .decl_type()
                .map(name_to_type)
                .unwrap_or(Ok(Type::UNKNOWN))?;
            Ok(FieldInfo::new(
                col.name().to_owned(),
                None,
                None,
                field_type,
                format.format_for(idx),
            ))
        })
        .collect()
}

fn name_to_type(name: &str) -> PgWireResult<Type> {
    dbg!(name);
    match name.to_uppercase().as_ref() {
        "INT" => Ok(Type::INT8),
        "VARCHAR" => Ok(Type::VARCHAR),
        "TEXT" => Ok(Type::TEXT),
        "BINARY" => Ok(Type::BYTEA),
        "FLOAT" => Ok(Type::FLOAT8),
        _ => Err(PgWireError::UserError(Box::new(ErrorInfo::new(
            "ERROR".to_owned(),
            "42846".to_owned(),
            format!("Unsupported data type: {name}"),
        )))),
    }
}

fn encode_row_data(
    rows: Rows,
    schema: Arc<Vec<FieldInfo>>,
) -> impl Stream<Item = PgWireResult<DataRow>> {
    stream::unfold((rows, schema), |(mut rows, schema)| async move {
        let row = rows.next().await.ok()??;

        let ncols = schema.len();
        let mut encoder = DataRowEncoder::new(schema.clone());

        for idx in 0..ncols {
            let data = row.get_value(idx as i32).unwrap();
            match data {
                Value::Null => encoder.encode_field(&None::<i8>).unwrap(),
                Value::Integer(i) => encoder.encode_field(&i).unwrap(),
                Value::Real(f) => encoder.encode_field(&f).unwrap(),
                Value::Text(t) => encoder
                    .encode_field(&String::from_utf8_lossy(t.as_bytes()).as_ref())
                    .unwrap(),
                Value::Blob(b) => encoder.encode_field(&b).unwrap(),
            }
        }

        Some((Ok(encoder.take_row()), (rows, schema)))
    })
}

#[allow(unused)]
fn get_database<C>(client:&C) -> PgWireResult<String>
    where C: ClientInfo + ClientPortalStore + Sink<PgWireBackendMessage> + Unpin + Send + Sync,
{
    let database = client.metadata().get(METADATA_DATABASE)
        .map_or_else(
            ||{
                Err(PgWireError::ApiError(Box::new(m_error!(EC::InternalErr, "Database not found"))))
            },
            |s| {
                Ok(s.to_string())
            })?;
    Ok(database)
}