//! Tests for `run_with_connectors` subcommand dispatch.

use super::*;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_contract::protocol::KeyValue;
use serde_json::json;

// ---------------------------------------------------------------------------
// run / command dispatch
// ---------------------------------------------------------------------------

#[test]
fn run_command_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let client = MockAsyncClient::new().with_command_response(select_1_response());
        let command = Commands::Command(json_args(r#"{"app_name":"demo","sql":"select 1"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector { client },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_put_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new().with_put_response(PutResponse::new(true));
        let command = Commands::Put(json_args(r#"{"key":"k","value":"v"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_get_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new()
            .with_get_response(GetResponse::new(Some(json_string_value_bytes("value-1"))));
        let command = Commands::Get(json_args(r#"{"key":"k"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_range_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client =
            MockAsyncClient::new().with_range_response(RangeScanResponse::new(vec![
                KeyValue::new(json_string_value_bytes("a"), json_string_value_bytes("1")),
            ]));
        let command = Commands::Range(json_args(r#"{"start_key":"a","end_key":"z"}"#));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_invoke_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let async_client = MockAsyncClient::new().with_invoke_procedure_response(
            ProcedureInvokeResponse::new(br#"{"ok":true}"#.to_vec()),
        );
        let command = Commands::Invoke(json_args(
            r#"{"procedure_name":"app/mod/proc","procedure_parameters":{}}"#,
        ));
        run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_command_connect_failure_is_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::Command(json_args(r#"{"app_name":"demo","sql":"select 1"}"#));
        let err = run_with_connectors(
            cli(command),
            &FailingJsonConnector,
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Network);
    })
    .unwrap();
}

// Starts a mock HTTP server; Miri's networking support is not reliable enough
// for this test, so run it natively.
#[cfg_attr(miri, ignore)]
#[test]
fn run_app_invoke_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {
                "proc_desc": {
                    "module_name": "wallet",
                    "proc_name": "balance",
                    "param_desc": {"fields": []},
                    "return_desc": {"fields": []},
                }
            }
        }));

        let result = procedure_invoke::serialize_result(Ok(ProcedureResult::new(vec![]))).unwrap();
        let async_client = MockAsyncClient::new()
            .with_invoke_procedure_response(ProcedureInvokeResponse::new(result));

        let mut c = cli(Commands::AppInvoke(AppInvokeArgs {
            app: "wallet".to_string(),
            module: "wallet".to_string(),
            proc: "balance".to_string(),
            request: json_args("{}"),
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: async_client,
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_install_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let path = temp_json_path("app_install.mpk");
        mudu_sys::fs::sync::sync_write(&path, b"fake mpk").unwrap();

        let mut c = cli(Commands::AppInstall(AppInstallArgs { mpk: path.clone() }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();

        let _ = mudu_sys::fs::sync::sync_remove_file(path);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_list_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": [{"name": "wallet"}]
        }));
        let mut c = cli(Commands::AppList);
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_detail_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
            "ok": true,
            "data": {"app_name": "wallet"}
        }));
        let mut c = cli(Commands::AppDetail(AppDetailArgs {
            app: "wallet".to_string(),
            module: None,
            proc: None,
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_app_uninstall_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let mut c = cli(Commands::AppUninstall(AppUninstallArgs {
            app: "wallet".to_string(),
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_server_topology_subcommand_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
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
        let mut c = cli(Commands::ServerTopology);
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_partition_route_with_key_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(json!({
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
        let mut c = cli(Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: Some(vec!["user-1".to_string()]),
            start: None,
            end: None,
        }));
        c.http_addr = http_addr;

        run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn run_partition_route_rejects_key_and_range_together() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: Some(vec!["user-1".to_string()]),
            start: Some(vec!["a".to_string()]),
            end: None,
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}

#[test]
fn run_partition_route_rejects_missing_key_and_range() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::PartitionRoute(PartitionRouteArgs {
            rule_name: "user_rule".to_string(),
            key: None,
            start: None,
            end: None,
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}

#[test]
fn run_app_detail_rejects_proc_without_module() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let command = Commands::AppDetail(AppDetailArgs {
            app: "wallet".to_string(),
            module: None,
            proc: Some("transfer".to_string()),
        });
        let err = run_with_connectors(
            cli(command),
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    })
    .unwrap();
}
