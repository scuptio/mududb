//! Failure-path tests for `run_with_connectors` dispatch: TCP connect
//! failures, session-create failures, argument validation, and management
//! error propagation (via connection-refused on a dead HTTP address).

use super::*;
use crate::cli::{
    AppDetailArgs, AppInstallArgs, AppUninstallArgs, BackupArgs, BackupCommands, BackupCreateArgs,
    BackupLabelArgs, DumpArgs, PartitionRouteArgs, PitrArgs, ReplAddNodeArgs, ReplArgs,
    ReplCommands, ReplFollowArgs, ReplNodeArgs, ReplPromoteArgs, RestoreArgs, RestoreLogicalArgs,
};

fn dead_http_cli(command: Commands) -> Cli {
    Cli {
        http_addr: "127.0.0.1:1".to_string(),
        ..cli(command)
    }
}

#[test]
fn command_request_failure_propagates() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // The mock client answers no command: the query error is wrapped.
        let err = run_with_connectors(
            cli(Commands::Command(json_args(
                r#"{"app_name":"demo","sql":"select 1"}"#,
            ))),
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
            err.to_string().contains("command request failed"),
            "unexpected: {err}"
        );
    })
    .unwrap();
}

#[test]
fn tcp_subcommand_connect_failures_are_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let cases = [
            Commands::Put(json_args(r#"{"key":"k","value":"v"}"#)),
            Commands::Get(json_args(r#"{"key":"k"}"#)),
            Commands::Range(json_args(r#"{"start_key":"a","end_key":"z"}"#)),
            Commands::Invoke(json_args(
                r#"{"app_name":"a","mod_name":"m","proc_name":"p","params":[]}"#,
            )),
        ];
        for command in cases {
            let err =
                run_with_connectors(cli(command), &FailingJsonConnector, &FailingAsyncConnector)
                    .await
                    .unwrap_err();
            assert!(
                err.to_string().contains("connect 127.0.0.1:9527 failed"),
                "unexpected: {err}"
            );
        }
    })
    .unwrap();
}

#[test]
fn tcp_subcommand_session_failures_are_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let cases: [(Commands, &str); 4] = [
            (
                Commands::Put(json_args(r#"{"key":"k","value":"v"}"#)),
                "session-create for put failed",
            ),
            (
                Commands::Get(json_args(r#"{"key":"k"}"#)),
                "session-create for get failed",
            ),
            (
                Commands::Range(json_args(r#"{"start_key":"a","end_key":"z"}"#)),
                "session-create for range failed",
            ),
            (
                Commands::Invoke(json_args(
                    r#"{"app_name":"a","mod_name":"m","proc_name":"p","params":[]}"#,
                )),
                "session-create for invoke failed",
            ),
        ];
        for (command, needle) in cases {
            let err = run_with_connectors(
                cli(command),
                &MockJsonConnector {
                    client: MockAsyncClient::new(),
                },
                &MockAsyncConnector {
                    client: MockAsyncClient::new().with_session_failure(),
                },
            )
            .await
            .unwrap_err();
            assert!(
                err.to_string().contains(needle),
                "expected '{needle}': {err}"
            );
        }
    })
    .unwrap();
}

#[test]
fn app_install_missing_package_is_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let err = run_with_connectors(
            cli(Commands::AppInstall(AppInstallArgs {
                mpk: PathBuf::from("/no/such/package.mpk"),
            })),
            &FailingJsonConnector,
            &FailingAsyncConnector,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("failed"), "unexpected: {err}");
    })
    .unwrap();
}

#[test]
fn management_subcommand_network_failures_are_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let commands = vec![
            Commands::AppList,
            Commands::AppDetail(AppDetailArgs {
                app: "a".to_string(),
                module: None,
                proc: None,
            }),
            Commands::AppUninstall(AppUninstallArgs {
                app: "a".to_string(),
            }),
            Commands::ServerTopology,
            Commands::PartitionRoute(PartitionRouteArgs {
                rule_name: "r".to_string(),
                key: Some(vec!["1".to_string()]),
                start: None,
                end: None,
            }),
            Commands::Dump(DumpArgs {
                label: "l".to_string(),
                dir: "/tmp/d".to_string(),
            }),
            Commands::RestoreLogical(RestoreLogicalArgs {
                label: "l".to_string(),
                dir: "/tmp/d".to_string(),
                drop: false,
            }),
        ];
        for command in commands {
            assert!(
                run_with_connectors(
                    dead_http_cli(command),
                    &FailingJsonConnector,
                    &FailingAsyncConnector
                )
                .await
                .is_err()
            );
        }
    })
    .unwrap();
}

#[test]
fn repl_subcommand_network_failures_are_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let commands = vec![
            Commands::Repl(ReplArgs {
                command: ReplCommands::AddNode(ReplAddNodeArgs {
                    node_id: 4,
                    host: "127.0.0.1".to_string(),
                    port: 9704,
                    learner: false,
                }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::AddNode(ReplAddNodeArgs {
                    node_id: 4,
                    host: "127.0.0.1".to_string(),
                    port: 9704,
                    learner: true,
                }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::RemoveNode(ReplNodeArgs { node_id: 4 }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::PromoteLearner(ReplNodeArgs { node_id: 4 }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::Promote(ReplPromoteArgs {
                    fencing_cmd: None,
                    yes: true,
                }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::Follow(ReplFollowArgs {
                    primary: "127.0.0.1:9701".to_string(),
                    primary_node_id: 1,
                }),
            }),
            Commands::Repl(ReplArgs {
                command: ReplCommands::Status,
            }),
        ];
        for command in commands {
            assert!(
                run_with_connectors(
                    dead_http_cli(command),
                    &FailingJsonConnector,
                    &FailingAsyncConnector
                )
                .await
                .is_err()
            );
        }
    })
    .unwrap();
}

#[test]
fn backup_subcommand_network_failures_are_reported() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let commands = vec![
            Commands::Backup(BackupArgs {
                command: BackupCommands::Create(BackupCreateArgs {
                    label: "b1".to_string(),
                }),
            }),
            Commands::Backup(BackupArgs {
                command: BackupCommands::List,
            }),
            Commands::Backup(BackupArgs {
                command: BackupCommands::Verify(BackupLabelArgs {
                    label: "b1".to_string(),
                }),
            }),
            Commands::Backup(BackupArgs {
                command: BackupCommands::Delete(BackupLabelArgs {
                    label: "b1".to_string(),
                }),
            }),
        ];
        for command in commands {
            assert!(
                run_with_connectors(
                    dead_http_cli(command),
                    &FailingJsonConnector,
                    &FailingAsyncConnector
                )
                .await
                .is_err()
            );
        }
    })
    .unwrap();
}

#[test]
fn pitr_requires_a_target() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let missing = PathBuf::from("/no/such/backup-dir");
        let err = run_with_connectors(
            cli(Commands::Pitr(PitrArgs {
                label: "b1".to_string(),
                target_dir: PathBuf::from("/no/such/target"),
                backup_dir: missing.to_string_lossy().into_owned(),
                archive_dir: missing.to_string_lossy().into_owned(),
                lsn: None,
                time: None,
                xid: None,
            })),
            &FailingJsonConnector,
            &FailingAsyncConnector,
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("pitr requires"),
            "unexpected: {err}"
        );
    })
    .unwrap();
}

#[test]
fn restore_and_pitr_report_backend_failures() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let nanos = mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let missing = mudu_sys::env_var::temp_dir()
            .join(format!("mcli-restore-missing-{nanos}"))
            .to_string_lossy()
            .into_owned();
        let err = run_with_connectors(
            cli(Commands::Restore(RestoreArgs {
                label: "b1".to_string(),
                target_dir: PathBuf::from(missing.clone()),
                backup_dir: missing.clone(),
                archive_dir: missing.clone(),
            })),
            &FailingJsonConnector,
            &FailingAsyncConnector,
        )
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("restore failed"),
            "unexpected: {err}"
        );

        let err = run_with_connectors(
            cli(Commands::Pitr(PitrArgs {
                label: "b1".to_string(),
                target_dir: PathBuf::from(missing.clone()),
                backup_dir: missing.clone(),
                archive_dir: missing,
                lsn: Some(7),
                time: None,
                xid: None,
            })),
            &FailingJsonConnector,
            &FailingAsyncConnector,
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("pitr failed"), "unexpected: {err}");
    })
    .unwrap();
}
