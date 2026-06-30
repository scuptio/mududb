//! Tests for the HTTP management API helpers in `management.rs`.
//!
//! These tests spin up a minimal local HTTP/1.1 server so the management
//! functions can be exercised without a real MuduDB server.

use crate::management::{
    PartitionRouteResponse, WorkerTopology, fetch_app_detail, fetch_app_list, fetch_proc_desc,
    fetch_server_topology, install_app_package, route_partition, uninstall_app,
};
use mudu::common::id::OID;
use serde_json::json;
use std::io::{Read, Write};

/// Start a minimal HTTP/1.1 server on a random local port and return its address.
///
/// The server always replies with `200 OK` and the JSON body produced by
/// `response_body`.  It parses the request headers and reads the request body
/// so that POST/DELETE requests are handled correctly.
fn start_mock_http_server(response_body: serde_json::Value) -> String {
    use mudu_sys::net::sync::bind_tcp;
    use std::net::SocketAddr;

    let listener = bind_tcp("127.0.0.1:0".parse::<SocketAddr>().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let body = serde_json::to_string(&response_body).unwrap();

    mudu_sys::task::sync::spawn_thread_named("mock-http", move || {
        let (mut socket, _) = listener.accept().unwrap();

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
    })
    .unwrap();

    addr
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
