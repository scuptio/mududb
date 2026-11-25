//! Tests for the HTTP management API helpers in `management.rs`.
//!
//! These tests spin up a minimal local HTTP/1.1 server so the management
//! functions can be exercised without a real MuduDB server.

use crate::management::{
    PartitionRouteResponse, WorkerTopology, backup_create, backup_delete, backup_list,
    backup_verify, dump_logical, fetch_app_detail, fetch_app_list, fetch_proc_desc,
    fetch_server_replication, fetch_server_topology, install_app_package, repl_follow,
    repl_membership, repl_promote, restore_logical, route_partition, uninstall_app,
};
use mudu::common::id::OID;
use serde_json::json;
use std::io::{Read, Write};

/// Spawn the mock server loop on `listener`, serving `body` forever.
///
/// The loop parses the request headers and reads the request body so that
/// POST/DELETE requests are handled correctly; it keeps listening after each
/// connection because the client retries failed requests.
fn spawn_mock_server(listener: mudu_sys::net::sync::StdTcpListener, body: String) -> String {
    let addr = listener.local_addr().unwrap().to_string();

    mudu_sys::task::sync::spawn_thread_named("mock-http", move || {
        loop {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => continue,
            };

            // Read headers up to the empty line.
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

    addr
}

fn bind_mock_listener() -> mudu_sys::net::sync::StdTcpListener {
    use mudu_sys::net::sync::bind_tcp;
    use std::net::SocketAddr;

    bind_tcp("127.0.0.1:0".parse::<SocketAddr>().unwrap()).unwrap()
}

/// Start a minimal HTTP/1.1 server on a random local port and return its address.
///
/// The server always replies with `200 OK` and the JSON body produced by
/// `response_body`.
fn start_mock_http_server(response_body: serde_json::Value) -> String {
    start_raw_http_server(serde_json::to_string(&response_body).unwrap())
}

/// Serve a raw response body (used to answer malformed JSON).
fn start_raw_http_server(body: String) -> String {
    spawn_mock_server(bind_mock_listener(), body)
}

/// The response body is computed from the listening port, so tests can point
/// redirect hints back at the mock itself.
fn start_mock_http_server_with_port(body_for: impl FnOnce(u16) -> serde_json::Value) -> String {
    let listener = bind_mock_listener();
    let port = listener.local_addr().unwrap().port();
    spawn_mock_server(listener, serde_json::to_string(&body_for(port)).unwrap())
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_app_list_returns_data() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": [{"name": "wallet"}, {"name": "kv"}]
        }));
        let list = fetch_app_list(&addr).await.unwrap();
        assert_eq!(list.as_array().unwrap().len(), 2);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_app_detail_without_module_returns_app_info() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {"app_name": "wallet", "modules": ["wallet"]},
        }));
        let detail = fetch_app_detail(&addr, "wallet", None, None).await.unwrap();
        assert_eq!(detail["app_name"], json!("wallet"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_app_detail_with_module_and_proc_returns_proc_info() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "proc_desc": {
                    "module_name": "wallet",
                    "proc_name": "transfer",
                    "param_desc": {"fields": []},
                    "return_desc": {"fields": []},
                }
            },
        }));
        let detail = fetch_app_detail(&addr, "wallet", Some("wallet"), Some("transfer"))
            .await
            .unwrap();
        assert_eq!(detail["proc_desc"]["proc_name"], json!("transfer"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_app_detail_rejects_proc_without_module() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let err = fetch_app_detail(&addr, "wallet", None, Some("transfer"))
            .await
            .unwrap_err();
        assert!(err.contains("--proc requires --module"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_proc_desc_extracts_procedure_descriptor() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "proc_desc": {
                    "module_name": "wallet",
                    "proc_name": "transfer",
                    "param_desc": {"fields": []},
                    "return_desc": {"fields": []},
                }
            },
        }));
        let desc = fetch_proc_desc(&addr, "wallet", "wallet", "transfer")
            .await
            .unwrap();
        assert_eq!(desc.proc_name(), "transfer");
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn fetch_server_topology_decodes_topology() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "worker_count": 1,
                "tcp_multi_port": false,
                "tcp_base_listen_port": 0,
                "workers": [
                    {
                        "worker_index": 0,
                        "tcp_listen_port": 9527,
                        "worker_id": {"h": 0, "l": 1},
                        "partitions": []
                    }
                ]
            }
        }));
        let topology = fetch_server_topology(&addr).await.unwrap();
        assert_eq!(topology.worker_count, 1);
        assert_eq!(topology.worker_port_by_index(0), Some(9527));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn install_app_package_posts_mpk_and_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({"ok": true, "data": null}));
        install_app_package(&addr, vec![1, 2, 3]).await.unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn uninstall_app_deletes_and_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({"ok": true, "data": null}));
        uninstall_app(&addr, "wallet").await.unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn route_partition_with_key_returns_routes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "routes": [
                    {
                        "partition_id": {"h": 0, "l": 1},
                        "worker_id": {"h": 0, "l": 2},
                    }
                ]
            }
        }));
        let response = route_partition(
            &addr,
            "user_rule",
            Some(vec!["user-1".to_string()]),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(response.routes.len(), 1);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn route_partition_with_range_returns_routes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "routes": [
                    {
                        "partition_id": {"h": 0, "l": 10},
                        "worker_id": {"h": 0, "l": 20},
                    }
                ]
            }
        }));
        let response = route_partition(
            &addr,
            "user_rule",
            None,
            Some(vec!["a".to_string()]),
            Some(vec!["z".to_string()]),
        )
        .await
        .unwrap();
        assert_eq!(response.routes.len(), 1);
    })
    .unwrap();
}

#[test]
fn partition_route_response_round_trips_oid() {
    let response: PartitionRouteResponse = serde_json::from_value(json!({
        "routes": [
            {
                "partition_id": {"h": 1, "l": 2},
                "worker_id": {"h": 3, "l": 4},
            }
        ]
    }))
    .unwrap();
    assert_eq!(
        response.routes[0].partition_id,
        OID::from(((1u128) << 64) + 2)
    );
    assert_eq!(response.routes[0].worker_id, OID::from(((3u128) << 64) + 4));
}

#[test]
fn worker_topology_defaults_missing_port_to_zero() {
    let worker: WorkerTopology = serde_json::from_value(json!({
        "worker_index": 0,
        "worker_id": {"h": 0, "l": 1},
        "partitions": []
    }))
    .unwrap();
    assert_eq!(worker.tcp_listen_port, 0);
}

#[cfg_attr(miri, ignore)]
#[test]
fn backup_dump_restore_and_replication_endpoints() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_mock_http_server(json!({"ok": true, "data": {"done": true}}));
        backup_create(&addr, "b1").await.unwrap();
        backup_list(&addr).await.unwrap();
        backup_verify(&addr, "b1").await.unwrap();
        backup_delete(&addr, "b1").await.unwrap();
        dump_logical(&addr, "l1", "/tmp/dump").await.unwrap();
        restore_logical(&addr, "l1", "/tmp/dump", true)
            .await
            .unwrap();
        fetch_server_replication(&addr).await.unwrap();
        repl_promote(&addr, true).await.unwrap();
        repl_follow(&addr, 2, "127.0.0.1", 9527).await.unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn api_error_envelope_and_legacy_status_form() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // `ok: false` envelope with an error message.
        let err_addr = start_mock_http_server(json!({
            "ok": false,
            "error": {"message": "denied", "code": 7}
        }));
        let err = fetch_app_list(&err_addr).await.unwrap_err();
        assert!(err.contains("denied"));

        // Legacy `status` form: 0 is success, non-zero is an error.
        let status_ok = start_mock_http_server(json!({"status": 0, "data": {"x": 1}}));
        assert_eq!(fetch_app_list(&status_ok).await.unwrap()["x"], json!(1));
        let status_err = start_mock_http_server(json!({"status": 9, "message": "legacy bad"}));
        let err = fetch_app_list(&status_err).await.unwrap_err();
        assert!(err.contains("legacy bad"));

        // Neither form present is a hard error.
        let garbage = start_mock_http_server(json!({"something": 1}));
        assert!(fetch_app_list(&garbage).await.is_err());
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn membership_redirect_loop_hits_the_redirect_cap() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // The mock always answers with a leader hint pointing at itself, so
        // the client must stop at the redirect cap.
        let addr = start_mock_http_server_with_port(|port| {
            json!({
                "ok": false,
                "error": {"leader_hint": {"host": "127.0.0.1", "http_port": port}}
            })
        });
        let err = repl_membership(
            &addr,
            "add_voter",
            7,
            Some("127.0.0.1".to_string()),
            Some(9527),
        )
        .await
        .unwrap_err();
        assert!(err.contains("redirected more than"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn membership_success_and_plain_error() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let ok_addr = start_mock_http_server(json!({"ok": true, "data": {"voters": [1]}}));
        let value = repl_membership(&ok_addr, "remove", 7, None, None)
            .await
            .unwrap();
        assert_eq!(value["voters"], json!([1]));

        let err_addr = start_mock_http_server(json!({
            "ok": false,
            "error": {"message": "not leader, no hint"}
        }));
        let err = repl_membership(&err_addr, "remove", 7, None, None)
            .await
            .unwrap_err();
        assert!(err.contains("not leader, no hint"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn request_to_dead_port_retries_and_reports_error_chain() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // The discard port refuses immediately; the client retries and then
        // reports both the first and the last failure.
        let err = fetch_app_list("127.0.0.1:9").await.unwrap_err();
        assert!(err.contains("failed after 5 attempts"));
        assert!(err.contains("first failed with:"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn invalid_json_body_is_a_decode_error() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let addr = start_raw_http_server("this is not json".to_string());
        let err = fetch_app_list(&addr).await.unwrap_err();
        assert!(err.contains("decode HTTP response"));
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn topology_route_and_proc_desc_decode_failures() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let bad_topology = start_mock_http_server(json!({"ok": true, "data": {"nope": true}}));
        let err = fetch_server_topology(&bad_topology).await.unwrap_err();
        assert!(err.contains("decode server topology failed"));

        let bad_routes = start_mock_http_server(json!({
            "ok": true,
            "data": {"routes": [{"bad": 1}]}
        }));
        let err = route_partition(&bad_routes, "rule", None, None, None)
            .await
            .unwrap_err();
        assert!(err.contains("decode partition route failed"));

        let no_desc = start_mock_http_server(json!({"ok": true, "data": {}}));
        let err = fetch_proc_desc(&no_desc, "wallet", "wallet", "transfer")
            .await
            .unwrap_err();
        assert!(err.contains("missing proc_desc"));
    })
    .unwrap();
}
