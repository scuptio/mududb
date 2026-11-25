//! Wire-level coverage of the remote Mudud backend: the sync client path, the
//! async session loop path, and the backend dispatch, all against a mock
//! server speaking the frame protocol.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{backend, config};
use mudu::common::result::RS;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_contract::protocol::{
    Frame, FrameHeader, GetResponse, HEADER_LEN, KeyValue, MessageType, PutResponse,
    RangeScanResponse, ServerResponse, SessionCloseResponse, SessionCreateResponse,
    encode_get_response, encode_put_response, encode_range_scan_response, encode_server_response,
    encode_session_close_response, encode_session_create_response,
};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::io::{Read, Write};

/// Start a mock mudud server: per connection, answers each request frame with
/// a fixed successful response. Returns the "host:port" address.
fn start_mock_mudud() -> String {
    let listener = mudu_sys::net::sync::bind_tcp("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    mudu_sys::task::sync::spawn_thread_named("mock-mudud", move || {
        loop {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            loop {
                let mut header = [0u8; HEADER_LEN];
                if socket.read_exact(&mut header).is_err() {
                    break;
                }
                let payload_len = FrameHeader::decode_header_bytes(&header)
                    .unwrap()
                    .payload_len() as usize;
                let mut body = vec![0u8; payload_len];
                if socket.read_exact(&mut body).is_err() {
                    break;
                }
                let mut bytes = Vec::from(header);
                bytes.extend_from_slice(&body);
                let frame = Frame::decode(&bytes).unwrap();
                let request_id = frame.header().request_id();
                let response = match frame.header().message_type() {
                    MessageType::SessionCreate => {
                        encode_session_create_response(request_id, &SessionCreateResponse::new(42))
                            .unwrap()
                    }
                    MessageType::Get => {
                        encode_get_response(request_id, &GetResponse::new(Some(b"v".to_vec())))
                            .unwrap()
                    }
                    MessageType::Put => {
                        encode_put_response(request_id, &PutResponse::new(true)).unwrap()
                    }
                    MessageType::RangeScan => encode_range_scan_response(
                        request_id,
                        &RangeScanResponse::new(vec![KeyValue::new(b"a".to_vec(), b"1".to_vec())]),
                    )
                    .unwrap(),
                    MessageType::SessionClose => {
                        encode_session_close_response(request_id, &SessionCloseResponse::new(true))
                            .unwrap()
                    }
                    _ => encode_server_response(
                        request_id,
                        &ServerResponse::new(TupleFieldDesc::new(vec![]), vec![], 1, None),
                    )
                    .unwrap(),
                };
                if socket.write_all(&response).is_err() {
                    break;
                }
            }
        }
    })
    .unwrap();
    addr
}

/// Runs `test` with MUDU_CONNECTION pointing at `tcp_addr`; the env var is
/// process-global, so serialize with the config test lock.
fn with_mudud_config(tcp_addr: &str, async_loop: bool, test: impl FnOnce()) {
    let _guard = config::test_guard();
    config::reset_db_path_override_for_test();
    let saved = mudu_sys::env_var::var("MUDU_CONNECTION");
    let suffix = if async_loop { "/app?async=1" } else { "/app" };
    mudu_sys::env_var::set_var("MUDU_CONNECTION", &format!("mudud://{tcp_addr}{suffix}"));
    test();
    match saved {
        Some(value) => mudu_sys::env_var::set_var("MUDU_CONNECTION", &value),
        None => mudu_sys::env_var::remove_var("MUDU_CONNECTION"),
    }
    config::reset_db_path_override_for_test();
}

fn stmt(sql: &str) -> SQLStmtText {
    SQLStmtText::new(sql.to_string())
}

#[test]
fn sync_client_path_over_mock_wire() {
    let addr = start_mock_mudud();
    with_mudud_config(&addr, false, || {
        let session = backend::mudu_open_argv(&UniSessionOpenArgv::new(0)).unwrap();
        backend::mudu_put(session, b"k", b"v").unwrap();
        assert_eq!(
            backend::mudu_get(session, b"k").unwrap(),
            Some(b"v".to_vec())
        );
        let items = backend::mudu_range(session, b"a", b"z").unwrap();
        assert_eq!(items.len(), 1);
        backend::mudu_command(session, &stmt("update t set x = 1"), &()).unwrap();
        backend::mudu_batch(session, &stmt("insert into t values (1)"), &()).unwrap();
        let rows = backend::mudu_query::<i32>(session, &stmt("select 1"), &()).unwrap();
        assert!(rows.next_record().unwrap().is_none());
        backend::mudu_close(session).unwrap();
        // Closing an unknown session fails.
        assert!(backend::mudu_close(session).is_err());
    });
}

#[test]
fn async_session_loop_path_over_mock_wire() {
    let addr = start_mock_mudud();
    with_mudud_config(&addr, true, || {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let session = backend::mudu_open_argv_async(&UniSessionOpenArgv::new(0))
                .await
                .unwrap();
            backend::mudu_put_async(session, b"k", b"v").await.unwrap();
            assert_eq!(
                backend::mudu_get_async(session, b"k").await.unwrap(),
                Some(b"v".to_vec())
            );
            let items = backend::mudu_range_async(session, b"a", b"z")
                .await
                .unwrap();
            assert_eq!(items.len(), 1);
            backend::mudu_command_async(session, &stmt("update t set x = 1"), &())
                .await
                .unwrap();
            backend::mudu_batch_async(session, &stmt("insert into t values (1)"), &())
                .await
                .unwrap();
            let rows = backend::mudu_query_async::<i32>(session, &stmt("select 1"), &())
                .await
                .unwrap();
            assert!(rows.next_record().unwrap().is_none());
            backend::mudu_close_async(session).await.unwrap();
            RS::Ok(())
        })
        .unwrap()
        .unwrap();
    });
}

#[test]
fn sync_api_bridged_over_the_async_session_loop() {
    let addr = start_mock_mudud();
    with_mudud_config(&addr, true, || {
        // The sync entry points bridge onto the async session loop when the
        // connection string sets `async=1`.
        let session = backend::mudu_open_argv(&UniSessionOpenArgv::new(0)).unwrap();
        backend::mudu_put(session, b"k", b"v").unwrap();
        assert_eq!(
            backend::mudu_get(session, b"k").unwrap(),
            Some(b"v".to_vec())
        );
        let items = backend::mudu_range(session, b"a", b"z").unwrap();
        assert_eq!(items.len(), 1);
        backend::mudu_command(session, &stmt("update t set x = 1"), &()).unwrap();
        backend::mudu_batch(session, &stmt("insert into t values (1)"), &()).unwrap();
        let rows = backend::mudu_query::<i32>(session, &stmt("select 1"), &()).unwrap();
        assert!(rows.next_record().unwrap().is_none());
        backend::mudu_close(session).unwrap();
    });
}
