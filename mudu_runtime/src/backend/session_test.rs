// libsql/SQLite FFI is not available under Miri, so exclude this module.
#[cfg(all(test, not(miri)))]
mod tests {
    use crate::backend::session::{
        DummyAuthSource, Session, encode_row_data, get_params, name_to_type, row_desc_from_stmt,
    };
    use crate::backend::session_ctx::SessionCtx;
    use bytes::Bytes;
    use futures::{Sink, StreamExt};
    use libsql::{Builder, Connection, Value};
    use pgwire::api::Type;
    use pgwire::api::auth::{AuthSource, LoginInfo};
    use pgwire::api::portal::{Format, Portal};
    use pgwire::api::query::{ExtendedQueryHandler, SimpleQueryHandler};
    use pgwire::api::results::DescribeResponse;
    use pgwire::api::stmt::StoredStatement;
    use pgwire::api::store::MemPortalStore;
    use pgwire::api::{ClientInfo, ClientPortalStore, DefaultClient, PgWireConnectionState};
    use pgwire::error::{PgWireError, PgWireResult};
    use pgwire::messages::PgWireBackendMessage;
    use pgwire::messages::ProtocolVersion;
    use pgwire::messages::response::TransactionStatus;
    use pgwire::messages::startup::SecretKey;
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::task::{Context, Poll};
    use tempfile::tempdir;

    async fn temp_connection() -> (tempfile::TempDir, Connection) {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().join("test.db");
        let db = Builder::new_local(&db_path)
            .build()
            .await
            .expect("build libsql db");
        let conn = db.connect().expect("connect to libsql db");
        (dir, conn)
    }

    #[test]
    fn name_to_type_maps_known_types() {
        assert_eq!(name_to_type("INT").unwrap(), Type::INT8);
        assert_eq!(name_to_type("VARCHAR").unwrap(), Type::VARCHAR);
        assert_eq!(name_to_type("TEXT").unwrap(), Type::TEXT);
        assert_eq!(name_to_type("BINARY").unwrap(), Type::BYTEA);
        assert_eq!(name_to_type("FLOAT").unwrap(), Type::FLOAT8);
    }

    #[test]
    fn name_to_type_unknown_returns_sqlstate_42846() {
        let err = name_to_type("UNKNOWN_TYPE").unwrap_err();
        match err {
            PgWireError::UserError(info) => {
                assert_eq!(info.code, "42846");
                assert!(info.message.contains("Unsupported data type"));
            }
            _ => panic!("expected UserError, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn row_desc_from_stmt_returns_expected_fields() {
        let (_dir, conn) = temp_connection().await;
        conn.execute(
            "CREATE TABLE test_desc (a INT, b FLOAT, c TEXT, d BLOB)",
            (),
        )
        .await
        .unwrap();
        let stmt = conn
            .prepare("SELECT a, b, c, d FROM test_desc")
            .await
            .unwrap();

        let fields = row_desc_from_stmt(&stmt, &Format::UnifiedText).unwrap();
        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].name(), "a");
        assert_eq!(fields[0].datatype(), &Type::INT8);
        assert_eq!(fields[1].name(), "b");
        assert_eq!(fields[1].datatype(), &Type::FLOAT8);
        assert_eq!(fields[2].name(), "c");
        assert_eq!(fields[2].datatype(), &Type::TEXT);
        assert_eq!(fields[3].name(), "d");
        assert_eq!(fields[3].datatype(), &Type::BYTEA);
    }

    #[tokio::test]
    async fn row_desc_from_stmt_falls_back_to_unknown_for_missing_decl_type() {
        let (_dir, conn) = temp_connection().await;
        conn.execute("CREATE TABLE no_decl(x)", ()).await.unwrap();
        let stmt = conn.prepare("SELECT x FROM no_decl").await.unwrap();

        let fields = row_desc_from_stmt(&stmt, &Format::UnifiedText).unwrap();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].datatype(), &Type::UNKNOWN);
    }

    fn decode_data_row(row: &pgwire::messages::data::DataRow) -> Vec<Option<Vec<u8>>> {
        let data = row.data.as_ref();
        let mut values = Vec::with_capacity(row.field_count as usize);
        let mut offset = 0usize;
        for _ in 0..row.field_count {
            let len = i32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]);
            offset += 4;
            if len < 0 {
                values.push(None);
            } else {
                let end = offset + len as usize;
                values.push(Some(data[offset..end].to_vec()));
                offset = end;
            }
        }
        values
    }

    #[tokio::test]
    async fn encode_row_data_encodes_various_values() {
        let (_dir, conn) = temp_connection().await;
        conn.execute(
            "CREATE TABLE test_row (a INT, b REAL, c TEXT, d BLOB, e INT)",
            (),
        )
        .await
        .unwrap();
        conn.execute(
            "INSERT INTO test_row (a, b, c, d, e) VALUES (?, ?, ?, ?, ?)",
            (
                42i64,
                314f64 / 100f64,
                "hello",
                vec![0x01u8, 0x02u8],
                Value::Null,
            ),
        )
        .await
        .unwrap();

        let stmt = conn
            .prepare("SELECT a, b, c, d, e FROM test_row")
            .await
            .unwrap();
        let schema = Arc::new(row_desc_from_stmt(&stmt, &Format::UnifiedText).unwrap());
        let rows = stmt.query(()).await.unwrap();

        let mut stream = std::pin::pin!(encode_row_data(rows, schema));
        let row = stream.next().await.unwrap().unwrap();
        assert_eq!(row.field_count, 5);

        let values = decode_data_row(&row);
        assert_eq!(values[0].as_deref(), Some(b"42".as_slice()));
        assert_eq!(values[1].as_deref(), Some(b"3.14".as_slice()));
        assert_eq!(values[2].as_deref(), Some(b"hello".as_slice()));
        assert!(values[3].is_some());
        assert!(values[4].is_none());

        assert!(stream.next().await.is_none());
    }

    fn make_portal(
        sql: &str,
        param_types: Vec<Option<Type>>,
        params: Vec<Option<Bytes>>,
    ) -> Portal<String> {
        let mut portal = Portal::new_cursor(
            String::new(),
            Arc::new(StoredStatement::new(
                String::new(),
                sql.to_string(),
                param_types,
            )),
        );
        portal.parameter_format = Format::UnifiedBinary;
        portal.parameters = params;
        portal.result_column_format = Format::UnifiedText;
        portal
    }

    fn i32_bytes(v: i32) -> Bytes {
        Bytes::from(v.to_be_bytes().to_vec())
    }

    fn f64_bytes(v: f64) -> Bytes {
        Bytes::from(v.to_be_bytes().to_vec())
    }

    fn bool_bytes(v: bool) -> Bytes {
        Bytes::from(vec![v as u8])
    }

    #[test]
    fn get_params_extracts_supported_types() {
        let portal = make_portal(
            "SELECT * FROM t WHERE a = ? AND b = ? AND c = ? AND d = ?",
            vec![
                Some(Type::BOOL),
                Some(Type::INT4),
                Some(Type::TEXT),
                Some(Type::FLOAT8),
            ],
            vec![
                Some(bool_bytes(true)),
                Some(i32_bytes(42)),
                Some(Bytes::from_static(b"hello")),
                Some(f64_bytes(2.5)),
            ],
        );

        let values = get_params(&portal).unwrap();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0], Value::Integer(1));
        assert_eq!(values[1], Value::Integer(42));
        assert_eq!(values[2], Value::Text("hello".to_string()));
        assert_eq!(values[3], Value::Real(2.5));
    }

    #[test]
    fn get_params_rejects_null_required_parameter() {
        let portal = make_portal(
            "SELECT * FROM t WHERE a = ?",
            vec![Some(Type::INT4)],
            vec![None],
        );

        let err = get_params(&portal).unwrap_err();
        match err {
            PgWireError::UserError(info) => {
                assert_eq!(info.code, "22023");
                assert!(info.message.contains("NULL int4 parameter"));
            }
            _ => panic!("expected UserError, got {:?}", err),
        }
    }

    #[test]
    fn get_params_rejects_unsupported_type() {
        let portal = make_portal(
            "SELECT * FROM t WHERE a = ?",
            vec![Some(Type::UUID)],
            vec![Some(Bytes::from_static(
                b"00000000-0000-0000-0000-000000000000",
            ))],
        );

        let err = get_params(&portal).unwrap_err();
        match err {
            PgWireError::UserError(info) => {
                assert_eq!(info.code, "0A000");
                assert!(info.message.contains("Unsupported parameter type"));
            }
            _ => panic!("expected UserError, got {:?}", err),
        }
    }

    struct FakeClient {
        info: DefaultClient<String>,
    }

    impl FakeClient {
        fn new() -> Self {
            Self {
                info: DefaultClient::new(SocketAddr::from(([127, 0, 0, 1], 5432)), false),
            }
        }
    }

    impl ClientInfo for FakeClient {
        fn socket_addr(&self) -> SocketAddr {
            self.info.socket_addr()
        }
        fn is_secure(&self) -> bool {
            self.info.is_secure()
        }
        fn protocol_version(&self) -> ProtocolVersion {
            self.info.protocol_version()
        }
        fn set_protocol_version(&mut self, version: ProtocolVersion) {
            self.info.set_protocol_version(version);
        }
        fn pid_and_secret_key(&self) -> (i32, SecretKey) {
            self.info.pid_and_secret_key()
        }
        fn set_pid_and_secret_key(&mut self, pid: i32, secret_key: SecretKey) {
            self.info.set_pid_and_secret_key(pid, secret_key);
        }
        fn state(&self) -> PgWireConnectionState {
            self.info.state()
        }
        fn set_state(&mut self, new_state: PgWireConnectionState) {
            self.info.set_state(new_state);
        }
        fn transaction_status(&self) -> TransactionStatus {
            self.info.transaction_status()
        }
        fn set_transaction_status(&mut self, new_status: TransactionStatus) {
            self.info.set_transaction_status(new_status);
        }
        fn metadata(&self) -> &HashMap<String, String> {
            self.info.metadata()
        }
        fn metadata_mut(&mut self) -> &mut HashMap<String, String> {
            self.info.metadata_mut()
        }
        fn session_extensions(&self) -> &pgwire::api::SessionExtensions {
            self.info.session_extensions()
        }
        fn sni_server_name(&self) -> Option<&str> {
            self.info.sni_server_name()
        }
        fn client_certificates<'a>(&self) -> Option<&[rustls_pki_types::CertificateDer<'a>]> {
            self.info.client_certificates()
        }
    }

    impl ClientPortalStore for FakeClient {
        type PortalStore = MemPortalStore<String>;
        fn portal_store(&self) -> &Self::PortalStore {
            &self.info.portal_store
        }
    }

    impl Sink<PgWireBackendMessage> for FakeClient {
        type Error = PgWireError;
        fn poll_ready(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<PgWireResult<()>> {
            Poll::Ready(Ok(()))
        }
        fn start_send(self: Pin<&mut Self>, _item: PgWireBackendMessage) -> PgWireResult<()> {
            Ok(())
        }
        fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<PgWireResult<()>> {
            Poll::Ready(Ok(()))
        }
        fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<PgWireResult<()>> {
            Poll::Ready(Ok(()))
        }
    }

    async fn temp_ctx() -> (tempfile::TempDir, SessionCtx, Connection) {
        let dir = tempdir().expect("temp dir");
        let path = dir.path().to_string_lossy().to_string();
        let ctx = SessionCtx::new(path);
        ctx.open(&"app".to_string()).await.unwrap();
        let conn = ctx.connection().await.unwrap();
        (dir, ctx, conn)
    }

    #[tokio::test]
    async fn session_ctx_opens_and_returns_connection() {
        let (_dir, _ctx, conn) = temp_ctx().await;
        let mut rows = conn.query("SELECT 1", ()).await.unwrap();
        assert!(rows.next().await.unwrap().is_some());
    }

    #[tokio::test]
    async fn dummy_auth_source_returns_password() {
        let (_dir, ctx, _conn) = temp_ctx().await;
        let auth = DummyAuthSource::new(ctx);
        let login = LoginInfo::new(Some("user"), Some("app"), "127.0.0.1".to_string());
        let password = auth.get_password(&login).await.unwrap();
        assert!(password.salt().is_some());
    }

    #[tokio::test]
    async fn dummy_auth_source_rejects_missing_user() {
        let (_dir, ctx, _conn) = temp_ctx().await;
        let auth = DummyAuthSource::new(ctx);
        let login = LoginInfo::new(None, Some("app"), "127.0.0.1".to_string());
        let err = auth.get_password(&login).await.unwrap_err();
        assert!(matches!(err, PgWireError::ApiError(_)));
    }

    #[tokio::test]
    async fn dummy_auth_source_rejects_missing_database() {
        let (_dir, ctx, _conn) = temp_ctx().await;
        let auth = DummyAuthSource::new(ctx);
        let login = LoginInfo::new(Some("user"), None, "127.0.0.1".to_string());
        let err = auth.get_password(&login).await.unwrap_err();
        assert!(matches!(err, PgWireError::ApiError(_)));
    }

    #[tokio::test]
    async fn session_simple_query_select_returns_rows() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT, b TEXT)", ())
            .await
            .unwrap();
        conn.execute("INSERT INTO t (a, b) VALUES (1, 'x')", ())
            .await
            .unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let responses = SimpleQueryHandler::do_query(&session, &mut client, "SELECT a, b FROM t")
            .await
            .unwrap();
        assert_eq!(responses.len(), 1);
    }

    #[tokio::test]
    async fn session_simple_query_execution_returns_tag() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT)", ()).await.unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let responses =
            SimpleQueryHandler::do_query(&session, &mut client, "INSERT INTO t (a) VALUES (1)")
                .await
                .unwrap();
        assert_eq!(responses.len(), 1);
    }

    #[tokio::test]
    async fn session_extended_query_describe_statement_returns_fields() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT, b TEXT)", ())
            .await
            .unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let stmt = StoredStatement::new(
            String::new(),
            "SELECT a, b FROM t WHERE a = $1".to_string(),
            vec![Some(Type::INT4)],
        );
        let response = session
            .do_describe_statement(&mut client, &stmt)
            .await
            .unwrap();
        assert_eq!(response.fields().len(), 2);
    }

    #[tokio::test]
    async fn session_extended_query_describe_portal_returns_fields() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT, b TEXT)", ())
            .await
            .unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let portal = make_portal(
            "SELECT a, b FROM t WHERE a = ?",
            vec![Some(Type::INT4)],
            vec![],
        );
        let response = session
            .do_describe_portal(&mut client, &portal)
            .await
            .unwrap();
        assert_eq!(response.fields().len(), 2);
    }

    #[tokio::test]
    async fn session_extended_query_select_with_params_returns_rows() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT, b TEXT)", ())
            .await
            .unwrap();
        conn.execute("INSERT INTO t (a, b) VALUES (1, 'x')", ())
            .await
            .unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let portal = make_portal(
            "SELECT a, b FROM t WHERE a = ?",
            vec![Some(Type::INT4)],
            vec![Some(Bytes::from_static(&[0, 0, 0, 1]))],
        );
        let response = ExtendedQueryHandler::do_query(&session, &mut client, &portal, 0)
            .await
            .unwrap();
        match response {
            pgwire::api::results::Response::Query(query_response) => {
                assert_eq!(query_response.row_schema().len(), 2);
            }
            _ => panic!("expected query response"),
        }
    }

    #[tokio::test]
    async fn session_extended_query_execution_with_params_returns_tag() {
        let (_dir, ctx, conn) = temp_ctx().await;
        conn.execute("CREATE TABLE t (a INT, b TEXT)", ())
            .await
            .unwrap();

        let session = Session::new(ctx);
        let mut client = FakeClient::new();
        let portal = make_portal(
            "INSERT INTO t (a, b) VALUES (?, ?)",
            vec![Some(Type::INT4), Some(Type::TEXT)],
            vec![
                Some(Bytes::from_static(&[0, 0, 0, 2])),
                Some(Bytes::from_static(b"y")),
            ],
        );
        let response = ExtendedQueryHandler::do_query(&session, &mut client, &portal, 0)
            .await
            .unwrap();
        match response {
            pgwire::api::results::Response::Execution(_) => {}
            _ => panic!("expected execution response"),
        }
    }
}
