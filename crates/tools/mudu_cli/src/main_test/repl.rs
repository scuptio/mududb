//! Tests for the `repl` subcommand family and replication status rendering.

use super::*;
use serde_json::json;

// ---------------------------------------------------------------------------
// repl subcommands
// ---------------------------------------------------------------------------

fn membership_ok_body() -> Value {
    json!({
        "ok": true,
        "data": {
            "members": [
                {"node_id": 1, "can_vote": true, "host": "127.0.0.1", "port": 19527},
                {"node_id": 4, "can_vote": false, "host": "127.0.0.1", "port": 19530},
            ],
            "conf_term": 3,
            "conf_version": 7,
        }
    })
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_add_node_learner_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(membership_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::AddNode(ReplAddNodeArgs {
                node_id: 4,
                host: "127.0.0.1".to_string(),
                port: 19530,
                learner: true,
            }),
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["conf_version"], 7);
        assert_eq!(output["members"][1]["can_vote"], false);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_remove_node_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(membership_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::RemoveNode(ReplNodeArgs { node_id: 4 }),
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["conf_term"], 3);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_promote_learner_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(membership_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::PromoteLearner(ReplNodeArgs { node_id: 4 }),
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
fn run_repl_membership_retries_on_leader_hint() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // The hinted leader (second server) answers successfully; the first
        // server answers not-leader with a hint pointing at it.
        let leader_addr = start_mock_http_server(membership_ok_body());
        let (leader_host, leader_port) = leader_addr.rsplit_once(':').unwrap();
        let follower_addr = start_mock_http_server(json!({
            "ok": false,
            "data": {"code": 100, "name": "InvalidState", "message": "not the raft leader", "source": "", "location": ""},
            "error": {
                "code": 100,
                "name": "InvalidState",
                "message": "not the raft leader (leader hint: 2)",
                "source": "",
                "location": "",
                "leader_hint": {"node_id": 2, "host": leader_host, "http_port": leader_port.parse::<u16>().unwrap()},
            },
            "status": 100,
            "message": "fail to update replication membership",
        }));
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::RemoveNode(ReplNodeArgs { node_id: 4 }),
        }));
        c.http_addr = follower_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["conf_version"], 7);
    })
    .unwrap();
}

fn role_ok_body() -> Value {
    json!({
        "ok": true,
        "data": {"role": "primary", "epoch": 2, "primary_node_id": 2}
    })
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_promote_with_yes_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(role_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Promote(ReplPromoteArgs {
                fencing_cmd: None,
                yes: true,
            }),
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["role"], "primary");
        assert_eq!(output["epoch"], 2);
    })
    .unwrap();
}

#[test]
fn run_repl_promote_requires_fencing_confirmation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Promote(ReplPromoteArgs {
                fencing_cmd: None,
                yes: false,
            }),
        }));
        let err = run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("--yes"), "unexpected error: {err}");
    })
    .unwrap();
}

#[test]
fn run_repl_promote_aborts_when_fencing_cmd_fails() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Promote(ReplPromoteArgs {
                fencing_cmd: Some("exit 1".to_string()),
                yes: false,
            }),
        }));
        let err = run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("fencing command"),
            "unexpected error: {err}"
        );
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_promote_with_fencing_cmd_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(role_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Promote(ReplPromoteArgs {
                fencing_cmd: Some("exit 0".to_string()),
                yes: false,
            }),
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["role"], "primary");
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_follow_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(role_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Follow(ReplFollowArgs {
                primary: "127.0.0.1:19527".to_string(),
                primary_node_id: 2,
            }),
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["primary_node_id"], 2);
    })
    .unwrap();
}

#[test]
fn run_repl_follow_rejects_bad_primary_address() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Follow(ReplFollowArgs {
                primary: "no-port-here".to_string(),
                primary_node_id: 2,
            }),
        }));
        let err = run_with_connectors(
            c,
            &MockJsonConnector {
                client: MockAsyncClient::new(),
            },
            &MockAsyncConnector {
                client: MockAsyncClient::new(),
            },
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("host:port"),
            "unexpected error: {err}"
        );
    })
    .unwrap();
}

fn status_ok_body() -> Value {
    json!({
        "ok": true,
        "data": {
            "mode": "stream",
            "node_id": 1,
            "role": "primary",
            "serving": true,
            "stream": {"epoch": 2, "primary_node_id": 1, "is_primary": true, "sync": false, "sync_timeout_ms": 10000},
            "workers": [{"worker_index": 0, "wal_next_lsn": 128, "snapshot": {"committed_lsn": 64, "taken_at_ms": 1, "dir_bytes": 4096}}],
            "subscribers": [{"node_id": 2, "worker_index": 0, "stored_lsn": 100, "source_lsn": 127, "lag_lsn": 27}],
            "storage": {"wal_bytes": 8192, "staging_bytes": 0, "raft_log_bytes": 0, "snapshot_bytes": 4096, "refreshed_at_ms": 1},
            "counters": {"sync_write_timeouts": 3, "snapshots_taken": 1, "wal_chunks_reclaimed": 2, "staging_chunks_reclaimed": 0, "snapshot_offers_served": 1, "snapshot_restores_completed": 0},
        }
    })
}

#[cfg_attr(miri, ignore)]
#[test]
fn run_repl_status_json_succeeds() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let http_addr = start_mock_http_server(status_ok_body());
        let mut c = cli(Commands::Repl(ReplArgs {
            command: ReplCommands::Status,
        }));
        c.http_addr = http_addr;

        let output = run_with_connectors(
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
        assert_eq!(output["mode"], "stream");
        assert_eq!(output["stream"]["epoch"], 2);
        assert_eq!(output["subscribers"][0]["lag_lsn"], 27);
        assert_eq!(output["counters"]["sync_write_timeouts"], 3);
    })
    .unwrap();
}

#[cfg_attr(miri, ignore)]
#[test]
fn render_repl_status_text_sections() {
    let data = status_ok_body()["data"].clone();
    let text = render_repl_status_text(&data);
    assert!(
        text.contains("mode: stream"),
        "missing head section: {text}"
    );
    assert!(text.contains("epoch=2"), "missing stream section: {text}");
    assert!(text.contains("== workers =="), "missing workers section");
    assert!(
        text.contains("== subscribers =="),
        "missing subscribers section"
    );
    assert!(
        text.contains("== storage (bytes) =="),
        "missing storage section"
    );
    assert!(text.contains("== counters =="), "missing counters section");
    assert!(
        text.contains("lag_lsn") && text.contains("27"),
        "missing lag row: {text}"
    );
    assert!(
        text.contains("sync_write_timeouts: 3"),
        "missing counter: {text}"
    );
}
