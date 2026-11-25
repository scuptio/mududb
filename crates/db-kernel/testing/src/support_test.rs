#[cfg(test)]
mod tests {
    use crate::support::*;
    use mudu_runtime::backend::mudud_cfg::ServerMode;
    use mudu_utils::notifier::NotifyWait;
    use std::io;
    use std::time::Duration;

    #[test]
    fn supports_server_mode_legacy_and_tokio_always_true() {
        assert!(supports_server_mode(ServerMode::Legacy));
        assert!(supports_server_mode(ServerMode::Tokio));
    }

    #[test]
    fn supports_server_mode_iouring_matches_sys() {
        assert_eq!(
            supports_server_mode(ServerMode::IOUring),
            mudu_sys::io_uring_available()
        );
    }

    #[test]
    fn is_permission_denied_true() {
        let io_err = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "denied", io_err);
        assert!(is_permission_denied(&err));
    }

    #[test]
    fn is_permission_denied_false() {
        let io_err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "missing", io_err);
        assert!(!is_permission_denied(&err));
    }

    #[test]
    fn is_permission_denied_false_with_no_source() {
        let err = mudu::mudu_error!(mudu::error::ErrorCode::Network, "no source");
        assert!(!is_permission_denied(&err));
    }

    #[test]
    fn temp_dir_contains_prefix_and_is_unique() {
        let a = temp_dir("test_prefix");
        let b = temp_dir("test_prefix");
        assert_ne!(a, b);
        let name_a = a.file_name().unwrap().to_str().unwrap();
        let name_b = b.file_name().unwrap().to_str().unwrap();
        assert!(name_a.starts_with("test_prefix-"));
        assert!(name_b.starts_with("test_prefix-"));
    }

    #[test]
    fn test_runtime_domain_lock_is_singleton() {
        let lock1 = test_runtime_domain_lock();
        let lock2 = test_runtime_domain_lock();
        assert!(std::ptr::eq(lock1, lock2));
    }

    #[test]
    fn test_listener_bind_local_yields_ephemeral_port() {
        // Shares the process-wide port space with the other port tests; see
        // the port_space_guard note in lib_test.rs.
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        assert!(port > 0);
    }

    #[test]
    fn into_inner_returns_listener() {
        // Shares the process-wide port space with the other port tests; see
        // the port_space_guard note in lib_test.rs.
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        let inner = listener.into_inner();
        assert_eq!(inner.local_addr().unwrap().port(), port);
    }

    #[test]
    fn wait_until_backend_ready_ok_when_notified() {
        let notify_wait = NotifyWait::new();
        let (notifier, waiter) = notify_wait.notify_wait();
        let handle = mudu_sys::task::sync::spawn_thread(move || {
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(10));
            notifier.notify_all();
        })
        .expect("spawn backend-ready test thread");
        let result = wait_until_backend_ready(waiter, "test-service", Duration::from_secs(5));
        handle.join().expect("join backend-ready test thread");
        assert!(result.is_ok());
    }

    #[test]
    fn wait_until_backend_ready_errors_on_timeout() {
        let notify_wait = NotifyWait::new();
        let (_notifier, waiter) = notify_wait.notify_wait();
        let result = wait_until_backend_ready(waiter, "test-service", Duration::from_millis(10));
        assert!(result.is_err());
    }

    // The debug server thread is intentionally detached; Miri requires all
    // threads to be joined, so skip this test under Miri.
    #[cfg_attr(miri, ignore)]
    #[test]
    fn start_debug_server_starts_without_error() {
        // The debug server keeps its listener bound for the rest of the
        // process, so it must not overlap the other port tests; see the
        // port_space_guard note in lib_test.rs.
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let listener = TestListener::bind_local().unwrap().expect("bind failed");
        let port = listener.port().unwrap();
        // Release the port so the debug server can bind to it.
        drop(listener);
        assert!(start_debug_server(port).is_ok());
    }

    #[test]
    fn temp_dir_domain_lock_and_startup_timeout() {
        let dir = temp_dir("myprefix");
        assert!(dir.to_string_lossy().contains("myprefix-"));
        let guard = test_runtime_domain_lock().lock().unwrap();
        drop(guard);
        let expected = if sanitizer_runtime_detected() {
            Duration::from_secs(600)
        } else if coverage_runtime_detected() {
            Duration::from_secs(120)
        } else {
            Duration::from_secs(30)
        };
        assert_eq!(backend_startup_timeout(), expected);
    }

    #[test]
    fn wait_until_backend_ready_resolves_and_times_out() {
        let (_notifier, waiter) = NotifyWait::new_notify_wait();
        assert!(
            wait_until_backend_ready(waiter, "svc", Duration::from_millis(50))
                .unwrap_err()
                .to_string()
                .contains("timed out")
        );

        let (notifier, waiter) = NotifyWait::new_notify_wait();
        notifier.notify_all();
        wait_until_backend_ready(waiter, "svc", Duration::from_secs(1)).unwrap();
    }

    #[test]
    fn port_readiness_waiters_detect_open_ports() {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let port = listener.port().unwrap();
        wait_until_async_port_ready(port, "test").unwrap();
        wait_until_worker_port_ready(port).unwrap();
        drop(listener);
    }

    use serde_json::json;
    use std::io::{Read, Write};

    /// Raw variant of the mock server: serves `body` verbatim (used for
    /// malformed-JSON decode error paths).
    fn start_mock_http_server_raw(body: String) -> u16 {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let port = listener.port().unwrap();
        mudu_sys::task::sync::spawn_thread_named("mock-http-raw", move || {
            let listener = listener.into_inner();
            loop {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => continue,
                };
                let mut sink = [0u8; 4096];
                let _ = socket.read(&mut sink);
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes());
            }
        })
        .unwrap();
        port
    }

    /// Start a minimal HTTP/1.1 server serving `body` forever; returns its port.
    fn start_mock_http_server(body: serde_json::Value) -> u16 {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let port = listener.port().unwrap();
        let body = serde_json::to_string(&body).unwrap();
        mudu_sys::task::sync::spawn_thread_named("mock-http", move || {
            let listener = listener.into_inner();
            loop {
                let (mut socket, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(_) => continue,
                };
                let mut header_buf = Vec::new();
                let mut byte = [0u8; 1];
                loop {
                    if socket.read_exact(&mut byte).is_err() {
                        break;
                    }
                    header_buf.push(byte[0]);
                    if header_buf.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let headers = String::from_utf8_lossy(&header_buf);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length: ")
                            .or_else(|| line.strip_prefix("content-length: "))
                    })
                    .and_then(|v| v.parse::<usize>().ok())
                    .unwrap_or(0);
                if content_length > 0 {
                    let mut body_buf = vec![0u8; content_length];
                    let _ = socket.read_exact(&mut body_buf);
                }
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes());
            }
        })
        .unwrap();
        port
    }

    #[test]
    fn fetch_post_delete_server_json_roundtrip() {
        let port = start_mock_http_server(json!({"status": 0, "data": {"k": 1}}));
        let value = fetch_server_json(port, "/mudu/x").unwrap();
        assert_eq!(value["data"]["k"], json!(1));
        let value = post_server_json(port, "/mudu/x", json!({"a": 2})).unwrap();
        assert_eq!(value["status"], json!(0));
        let value = delete_server_json(port, "/mudu/x").unwrap();
        assert_eq!(value["status"], json!(0));

        // Queue endpoint returns data when status is zero and fails otherwise.
        let queues = fetch_server_queues(port).unwrap();
        assert_eq!(queues["k"], json!(1));
        let bad_port = start_mock_http_server(json!({"status": 3, "message": "bad"}));
        assert!(fetch_server_queues(bad_port).is_err());
    }

    #[test]
    fn replication_topology_fetch_and_leader_wait() {
        let port = start_mock_http_server(json!({
            "data": {
                "replication": {
                    "node_id": 7,
                    "role": "Leader",
                    "term": 3,
                    "leader_node_id": 7,
                    "serving": true,
                    "commit_index": 42
                }
            }
        }));
        let view = fetch_replication_topology(port).unwrap().unwrap();
        assert_eq!(view.node_id, 7);
        assert!(view.serving);
        let leader = wait_until_leader_serving(&[(1, port)], Duration::from_secs(2)).unwrap();
        assert_eq!(leader, 7);

        // A single-node server has no replication section.
        let none_port = start_mock_http_server(json!({"data": {}}));
        assert!(fetch_replication_topology(none_port).unwrap().is_none());

        // No serving leader on a dead port before the deadline.
        assert!(wait_until_leader_serving(&[(1, 9)], Duration::from_millis(200)).is_err());
    }

    /// Start a minimal MSSP server: reads frames and answers SessionCreate
    /// with a fixed session id, everything else with an empty OK response.
    fn start_mock_mssp_server() -> u16 {
        use mudu_contract::protocol::{
            Frame, FrameHeader, HEADER_LEN, MessageType, ServerResponse, SessionCreateResponse,
            encode_server_response, encode_session_create_response,
        };
        use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

        let listener = TestListener::bind_local().unwrap().unwrap();
        let port = listener.port().unwrap();
        mudu_sys::task::sync::spawn_thread_named("mock-mssp", move || {
            let listener = listener.into_inner();
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
                        MessageType::SessionCreate => encode_session_create_response(
                            request_id,
                            &SessionCreateResponse::new(42),
                        )
                        .unwrap(),
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
        port
    }

    #[test]
    fn sync_retry_and_blocking_async_client_against_mock_mssp() {
        let port = start_mock_mssp_server();

        // The synchronous client with retry: connect + create session.
        let (mut client, session) =
            connect_session_with_retry(port, Duration::from_secs(5)).unwrap();
        assert_eq!(session, 42);
        let response = client.execute("app", "update t set x = 1").unwrap();
        assert_eq!(response.affected_rows(), 1);

        // The blocking wrapper over the async transport holds one runtime for
        // its lifetime; connect alone exercises the retry path.
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        assert!(BlockingAsyncClient::connect(addr).is_ok());
    }
    #[cfg(unix)]
    fn write_fake_mudud(tag: &str, body: &str) -> std::path::PathBuf {
        let dir = crate::support::temp_dir("fake_mudud");
        let _ = mudu_sys::fs::sync::sync_create_dir_all(&dir);
        let script = dir.join(format!("fake-mudud-{tag}.sh"));
        mudu_sys::fs::sync::sync_write(&script, body).unwrap();
        #[allow(clippy::disallowed_methods)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(&script).unwrap().permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&script, perms).unwrap();
        }
        script
    }

    #[cfg(unix)]
    #[test]
    fn mudud_child_lifecycle_and_wait_timeout_with_fake_binary() {
        use crate::support::MududChild;

        // A fake mudud that announces itself and idles: spawn works, the
        // log drain captures its output, wait_ready times out on a closed
        // port, and sigkill reaps it with signal 9.
        let script = write_fake_mudud("idle", "#!/bin/sh\necho fake-mudud-ready\nexec sleep 60\n");
        let cfg_dir = crate::support::temp_dir("fake_mudud_cfg");
        let _ = mudu_sys::fs::sync::sync_create_dir_all(&cfg_dir);
        let cfg = cfg_dir.join("mududb.toml");
        mudu_sys::fs::sync::sync_write(&cfg, "").unwrap();
        let mut child = MududChild::spawn(&script, &cfg).unwrap();
        let err = child
            .wait_ready(&[49_991], Duration::from_millis(300))
            .unwrap_err();
        assert!(err.to_string().contains("did not become ready"));
        // The drain threads append asynchronously; poll for the marker.
        let mut drained = String::new();
        for _ in 0..100 {
            drained = child.logs();
            if drained.contains("fake-mudud-ready") {
                break;
            }
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(20));
        }
        assert!(drained.contains("fake-mudud-ready"), "logs: {drained}");
        child.sigkill().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn mudud_child_wait_ready_reports_early_exit() {
        use crate::support::MududChild;

        let script = write_fake_mudud("exit", "#!/bin/sh\necho dying-now\nexit 1\n");
        let cfg_dir = crate::support::temp_dir("fake_mudud_cfg");
        let _ = mudu_sys::fs::sync::sync_create_dir_all(&cfg_dir);
        let cfg = cfg_dir.join("mududb.toml");
        mudu_sys::fs::sync::sync_write(&cfg, "").unwrap();
        let mut child = MududChild::spawn(&script, &cfg).unwrap();
        let err = child
            .wait_ready(&[49_992], Duration::from_secs(5))
            .unwrap_err();
        assert!(err.to_string().contains("exited during startup"));
        // The drain threads append asynchronously; poll for the marker.
        let mut drained = String::new();
        for _ in 0..100 {
            drained = child.logs();
            if drained.contains("dying-now") {
                break;
            }
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(20));
        }
        assert!(drained.contains("dying-now"), "logs: {drained}");
    }

    #[test]
    fn wait_until_leader_serving_finds_leader_and_times_out() {
        use crate::support::wait_until_leader_serving;

        let leader_port = start_mock_http_server(json!({
            "status": 0,
            "data": {
                "replication": {
                    "node_id": 7,
                    "role": "Leader",
                    "term": 3,
                    "leader_node_id": 7,
                    "serving": true,
                    "commit_index": 42
                }
            }
        }));
        let leader =
            wait_until_leader_serving(&[(7, leader_port)], Duration::from_secs(2)).unwrap();
        assert_eq!(leader, 7);

        // A non-serving follower never resolves.
        let follower_port = start_mock_http_server(json!({
            "status": 0,
            "data": {
                "replication": {
                    "node_id": 8,
                    "role": "Follower",
                    "term": 3,
                    "leader_node_id": 7,
                    "serving": false,
                    "commit_index": 42
                }
            }
        }));
        let err = wait_until_leader_serving(&[(8, follower_port)], Duration::from_millis(200))
            .unwrap_err();
        assert!(err.to_string().contains("no serving raft leader"));
    }
    #[test]
    fn http_helpers_report_error_status_bad_json_and_refused_ports() {
        use crate::support::{
            delete_server_json, fetch_server_json, fetch_server_queues, post_server_json,
        };
        // Non-zero status envelope fails the queues helper.
        let port = start_mock_http_server(json!({"status": 3, "message": "boom"}));
        let err = fetch_server_queues(port).unwrap_err();
        assert!(err.to_string().contains("queues endpoint failed"));

        // Malformed JSON body fails the decode step of every verb.
        let raw_port = start_mock_http_server_raw("this is not json".to_string());
        let err = fetch_server_json(raw_port, "/x").unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);
        let err = post_server_json(raw_port, "/x", json!({})).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);
        let err = delete_server_json(raw_port, "/x").unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);

        // A closed port fails the send step.
        let listener = crate::support::TestListener::bind_local().unwrap().unwrap();
        let closed = listener.port().unwrap();
        drop(listener);
        let err = fetch_server_json(closed, "/x").unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Network);
    }

    #[cfg(unix)]
    #[test]
    fn mudud_child_spawn_failure_is_an_error() {
        use crate::support::MududChild;

        let missing = std::path::PathBuf::from("/nonexistent/mudud-binary");
        let cfg = std::path::PathBuf::from("/nonexistent/mududb.toml");
        let err = MududChild::spawn(&missing, &cfg).err().unwrap();
        assert!(err.to_string().contains("spawn mudud binary"));
    }

    #[test]
    fn ensure_mudud_binary_honors_absolute_target_dir_env() {
        use crate::support::ensure_mudud_binary;

        // Point CARGO_TARGET_DIR at a temp dir with a marker `mudud` file:
        // the helper takes the env path and returns it without building.
        let dir = crate::support::temp_dir("mudud_env");
        let profile_dir = dir.join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        });
        mudu_sys::fs::sync::sync_create_dir_all(&profile_dir).unwrap();
        let fake = profile_dir.join("mudud");
        mudu_sys::fs::sync::sync_write(&fake, b"fake").unwrap();
        let dir_str = dir.to_string_lossy().to_string();
        let prev = mudu_sys::env_var::var("CARGO_TARGET_DIR");
        mudu_sys::env_var::set_var("CARGO_TARGET_DIR", &dir_str);
        let got = ensure_mudud_binary().unwrap();
        assert_eq!(got, fake);
        match prev {
            Some(v) => mudu_sys::env_var::set_var("CARGO_TARGET_DIR", &v),
            None => mudu_sys::env_var::remove_var("CARGO_TARGET_DIR"),
        }
    }

    #[test]
    fn blocking_async_client_full_api_against_mock_mssp() {
        let port = start_mock_mssp_server();
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let mut client = BlockingAsyncClient::connect(addr).unwrap();

        // The mock answers SessionCreate with id 42 and every other frame
        // with an empty OK response carrying affected_rows = 1.
        let session = client.create_session(None).unwrap();
        assert_eq!(session, 42);
        let session = client.create_session(Some("{}".to_string())).unwrap();
        assert_eq!(session, 42);

        let response = client.execute("app", "update t set x = 1").unwrap();
        assert_eq!(response.affected_rows(), 1);
        let response = client
            .execute_with_oid(42, "app", "update t set x = 2")
            .unwrap();
        assert_eq!(response.affected_rows(), 1);
        let response = client
            .execute_with_oid_params(42, "app", "update t set x = ?", vec![])
            .unwrap();
        assert_eq!(response.affected_rows(), 1);
        let response = client.query_with_oid(42, "app", "select 1").unwrap();
        assert_eq!(response.affected_rows(), 1);
        let response = client
            .query_with_oid_params(42, "app", "select ?", vec![])
            .unwrap();
        assert_eq!(response.affected_rows(), 1);
    }

    #[test]
    fn blocking_async_client_connect_times_out_on_closed_port() {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let closed = listener.port().unwrap();
        drop(listener);
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], closed));
        // connect retries on a fixed 5 s deadline before giving up.
        let err = BlockingAsyncClient::connect(addr).err().unwrap();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Network);
    }

    #[test]
    fn connect_session_with_retry_times_out_on_closed_port() {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let closed = listener.port().unwrap();
        drop(listener);
        let err = connect_session_with_retry(closed, Duration::from_millis(300))
            .err()
            .unwrap();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Network);
    }

    #[test]
    fn post_and_delete_report_refused_port() {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let closed = listener.port().unwrap();
        drop(listener);
        let err = post_server_json(closed, "/x", json!({})).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Network);
        let err = delete_server_json(closed, "/x").unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Network);
    }

    #[test]
    fn replication_topology_reports_undecodable_responses() {
        // A non-JSON body fails the envelope decode.
        let raw_port = start_mock_http_server_raw("this is not json".to_string());
        let err = fetch_replication_topology(raw_port).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);

        // A malformed replication section fails the view parse.
        let bad_port = start_mock_http_server(json!({"data": {"replication": 123}}));
        let err = fetch_replication_topology(bad_port).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);
    }

    #[cfg(unix)]
    #[test]
    fn mudud_child_sigkill_rejects_clean_exit() {
        use crate::support::MududChild;

        let script = write_fake_mudud("clean", "#!/bin/sh\nexit 0\n");
        let cfg_dir = crate::support::temp_dir("fake_mudud_cfg");
        let _ = mudu_sys::fs::sync::sync_create_dir_all(&cfg_dir);
        let cfg = cfg_dir.join("mududb.toml");
        mudu_sys::fs::sync::sync_write(&cfg, "").unwrap();
        let mut child = MududChild::spawn(&script, &cfg).unwrap();
        // Let the child exit on its own: it stays a zombie until reaped, so
        // kill() succeeds but wait() reports a plain exit status, not SIGKILL.
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(300));
        let err = child.sigkill().unwrap_err();
        assert!(err.to_string().contains("must die from SIGKILL"));
    }

    /// Reserves a port and releases it, so probes against it keep failing.
    #[cfg(not(feature = "ds"))]
    fn reserve_then_release_port() -> u16 {
        let listener = TestListener::bind_local().unwrap().unwrap();
        let port = listener.port().unwrap();
        drop(listener);
        port
    }

    #[cfg(not(feature = "ds"))]
    #[test]
    fn wait_until_ports_bindable_reports_busy_and_released_ports() {
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        // A held listener keeps its port busy: the wait times out on it.
        let held = TestListener::bind_local().unwrap().unwrap();
        let busy_port = held.port().unwrap();
        let err = wait_until_ports_bindable(&[busy_port], Duration::from_millis(150)).unwrap_err();
        assert!(err.to_string().contains("still busy"));
        // Once released, the port is bindable and the wait returns.
        drop(held);
        wait_until_ports_bindable(&[busy_port], Duration::from_secs(5)).unwrap();
    }

    #[cfg(not(feature = "ds"))]
    #[test]
    fn wait_test_server_ready_ok_when_ports_open_and_ready_fires() {
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        // Two real listeners stand in for the server's HTTP and worker ports.
        let http = TestListener::bind_local().unwrap().unwrap();
        let tcp = TestListener::bind_local().unwrap().unwrap();
        let (stop, stop_waiter) = NotifyWait::new_notify_wait();
        let (ready, _ready_waiter) = NotifyWait::new_notify_wait();
        let server_ready = ready.clone();
        let server = mudu_sys::task::sync::spawn_thread(move || {
            server_ready.notify_all();
            let _ = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                stop_waiter.wait().await;
            });
            Ok(())
        })
        .unwrap();
        let server = wait_test_server_ready(
            http.port().unwrap(),
            tcp.port().unwrap(),
            &stop,
            &ready,
            server,
            Duration::from_secs(5),
        )
        .unwrap();
        stop.notify_all();
        let outcome = server.join().expect("join mock server thread");
        assert!(outcome.is_ok());
    }

    #[cfg(not(feature = "ds"))]
    #[test]
    fn wait_test_server_ready_fails_fast_with_the_server_threads_error() {
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let http_port = reserve_then_release_port();
        let tcp_port = reserve_then_release_port();
        let (stop, _stop_waiter) = NotifyWait::new_notify_wait();
        let (ready, _ready_waiter) = NotifyWait::new_notify_wait();
        let server = mudu_sys::task::sync::spawn_thread(|| {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "bind worker listener: address already in use"
            ))
        })
        .unwrap();
        // The huge budget proves the wait does not poll until the deadline:
        // the thread's exit ends it immediately.
        let err = wait_test_server_ready(
            http_port,
            tcp_port,
            &stop,
            &ready,
            server,
            Duration::from_secs(3600),
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("exited during startup"));
        // The thread's own error is preserved in the source chain.
        let mut chain = Vec::new();
        let mut source = std::error::Error::source(&err);
        while let Some(cause) = source {
            chain.push(cause.to_string());
            source = cause.source();
        }
        assert!(
            chain
                .iter()
                .any(|cause| cause.contains("address already in use")),
            "error source chain: {chain:?}"
        );
    }

    #[cfg(not(feature = "ds"))]
    #[test]
    // The test panics a thread on purpose to check that the wait reports it;
    // the crate warns on clippy::panic, so scope the allow to this test.
    #[allow(clippy::panic)]
    fn wait_test_server_ready_reports_server_thread_panics() {
        use mudu::common::result::RS;

        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let http_port = reserve_then_release_port();
        let tcp_port = reserve_then_release_port();
        let (stop, _stop_waiter) = NotifyWait::new_notify_wait();
        let (ready, _ready_waiter) = NotifyWait::new_notify_wait();
        let server =
            mudu_sys::task::sync::spawn_thread(|| -> RS<()> { panic!("boom in server startup") })
                .unwrap();
        let err = wait_test_server_ready(
            http_port,
            tcp_port,
            &stop,
            &ready,
            server,
            Duration::from_secs(3600),
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("panicked during startup"));
        assert!(err.to_string().contains("boom in server startup"));
    }

    #[cfg(not(feature = "ds"))]
    #[test]
    fn wait_test_server_ready_timeout_signals_stop() {
        let _guard = test_runtime_domain_lock()
            .lock()
            .expect("test runtime domain lock poisoned");
        let http_port = reserve_then_release_port();
        let tcp_port = reserve_then_release_port();
        let (stop, stop_waiter) = NotifyWait::new_notify_wait();
        let (ready, _ready_waiter) = NotifyWait::new_notify_wait();
        let server = mudu_sys::task::sync::spawn_thread(move || {
            let _ = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                stop_waiter.wait().await;
            });
            Ok(())
        })
        .unwrap();
        let err = wait_test_server_ready(
            http_port,
            tcp_port,
            &stop,
            &ready,
            server,
            Duration::from_millis(150),
        )
        .err()
        .unwrap();
        assert!(err.to_string().contains("did not become ready"));
        // The helper fired stop so the still-running thread shuts down
        // instead of leaking its listeners.
        assert!(stop.is_notified());
    }
}
