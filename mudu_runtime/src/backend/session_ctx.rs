use crate::backend::session::{DummyAuthSource, Session};
use crate::db_connector::DBConnector;
use libsql::Connection;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

use mudu_utils::sync::a_mutex::AMutex;
use pgwire::api::auth::md5pass::Md5PasswordAuthStartupHandler;
use pgwire::api::auth::{DefaultServerParameterProvider, StartupHandler};
use pgwire::api::copy::CopyHandler;
use pgwire::api::query::{ExtendedQueryHandler, SimpleQueryHandler};
use pgwire::api::{ErrorHandler, NoopHandler, PgWireServerHandlers};
use std::sync::Arc;

#[derive(Clone)]
pub struct SessionCtx {
    db_path: String,
    inner: Arc<AMutex<Option<SessionCtxInner>>>,
}

struct SessionCtxInner {
    conn:Connection,
    _app_name: String,
}


impl SessionCtx {
    pub fn new(db_path: String) -> Self {
        Self {
            db_path,
            inner: Arc::new(AMutex::new(None))
        }
    }

    pub async fn open(&self, app_name: &String) -> RS<()> {
        let mut inner = self.inner.lock().await;
        *inner = Some(SessionCtxInner::open(&self.db_path, app_name).await?);
        Ok(())
    }

    pub async fn connection(&self) -> RS<Connection> {
        let inner = self.inner.lock().await;
        inner.as_ref().map_or_else(
            || { Err(m_error!(EC::NoneErr)) },
            |inner| { Ok(inner.conn.clone()) })
    }
}


impl SessionCtxInner {
    async fn open(db_path: &String, app_name: &String) -> RS<Self> {
        let conn_str = format!("db={} app={} db_type=LibSQL", db_path, app_name);
        let db_conn = DBConnector::connect(&conn_str).await?;
        let libsql_conn = DBConnector::get_libsql_conn(db_conn.expected_sync()?)
            .map_or_else(|| { Err(m_error!(EC::NoneErr)) }, |c| { Ok(c) })?;
        Ok(Self {
            conn: libsql_conn,
            _app_name: app_name.clone(),
        })
    }
}



impl PgWireServerHandlers for SessionCtx {
    fn simple_query_handler(&self) -> Arc<impl SimpleQueryHandler> {
        Arc::new(Session::new(self.clone()))
    }

    fn extended_query_handler(&self) -> Arc<impl ExtendedQueryHandler> {
        Arc::new(Session::new(self.clone()))
    }

    fn startup_handler(&self) -> Arc<impl StartupHandler> {
        Arc::new(Md5PasswordAuthStartupHandler::new(
            Arc::new(DummyAuthSource::new(self.clone())),
            Arc::new(DefaultServerParameterProvider::default()),
        ))
    }

    fn copy_handler(&self) -> Arc<impl CopyHandler> {
        Arc::new(NoopHandler)
    }

    fn error_handler(&self) -> Arc<impl ErrorHandler> {
        Arc::new(NoopHandler)
    }
}
