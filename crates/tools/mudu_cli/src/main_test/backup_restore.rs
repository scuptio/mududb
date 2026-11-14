//! Tests for the backup/restore/pitr/dump subcommand dispatch paths.

use super::*;
use serde_json::json;
use std::io::IsTerminal;

fn empty_connectors() -> (MockJsonConnector, MockAsyncConnector) {
    (
        MockJsonConnector {
            client: MockAsyncClient::new(),
        },
        MockAsyncConnector {
            client: MockAsyncClient::new(),
        },
    )
}

#[test]
fn run_backup_subcommands_succeed_over_http() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (json, async_) = empty_connectors();
        for command in [
            BackupCommands::Create(BackupCreateArgs {
                label: "b1".to_string(),
            }),
            BackupCommands::List,
            BackupCommands::Verify(BackupLabelArgs {
                label: "b1".to_string(),
            }),
            BackupCommands::Delete(BackupLabelArgs {
                label: "b1".to_string(),
            }),
        ] {
            // Each subcommand issues exactly one HTTP request.
            let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
            let mut c = cli(Commands::Backup(BackupArgs { command }));
            c.http_addr = http_addr;
            run_with_connectors(c, &json, &async_).await.unwrap();
        }
    })
    .unwrap();
}

#[test]
fn run_dump_and_restore_logical_subcommands_succeed_over_http() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (json, async_) = empty_connectors();

        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let mut c = cli(Commands::Dump(DumpArgs {
            label: "d1".to_string(),
            dir: "/tmp/dump".to_string(),
        }));
        c.http_addr = http_addr;
        run_with_connectors(c, &json, &async_).await.unwrap();

        let http_addr = start_mock_http_server(json!({"ok": true, "data": null}));
        let mut c = cli(Commands::RestoreLogical(RestoreLogicalArgs {
            label: "d1".to_string(),
            dir: "/tmp/dump".to_string(),
            drop: true,
        }));
        c.http_addr = http_addr;
        run_with_connectors(c, &json, &async_).await.unwrap();
    })
    .unwrap();
}

#[test]
fn run_restore_reports_missing_backup() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (json, async_) = empty_connectors();
        let dir = temp_json_path("restore_target");
        let c = cli(Commands::Restore(RestoreArgs {
            label: "absent".to_string(),
            target_dir: dir,
            backup_dir: "/tmp/no-such-backup-dir".to_string(),
            archive_dir: "/tmp/no-such-archive-dir".to_string(),
        }));
        let err = run_with_connectors(c, &json, &async_).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    })
    .unwrap();
}

#[test]
fn run_pitr_requires_a_target_and_reports_missing_backup() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (json, async_) = empty_connectors();
        let dir = temp_json_path("pitr_target");
        let base = PitrArgs {
            label: "absent".to_string(),
            target_dir: dir.clone(),
            backup_dir: "/tmp/no-such-backup-dir".to_string(),
            archive_dir: "/tmp/no-such-archive-dir".to_string(),
            lsn: None,
            time: None,
            xid: None,
        };

        // No --lsn/--time/--xid: rejected before touching disk.
        let c = cli(Commands::Pitr(base));
        let err = run_with_connectors(c, &json, &async_).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);

        // Each target variant reaches the restore path (which then fails on
        // the missing backup).
        for (lsn, time, xid) in [
            (Some(7), None, None),
            (None, Some(9), None),
            (None, None, Some(3)),
        ] {
            let args = PitrArgs {
                label: "absent".to_string(),
                target_dir: dir.clone(),
                backup_dir: "/tmp/no-such-backup-dir".to_string(),
                archive_dir: "/tmp/no-such-archive-dir".to_string(),
                lsn,
                time,
                xid,
            };
            let c = cli(Commands::Pitr(args));
            let err = run_with_connectors(c, &json, &async_).await.unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Database);
        }
    })
    .unwrap();
}

#[test]
fn real_connectors_report_connect_errors() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        // Nothing listens here: both real connectors must fail fast.
        assert!(RealJsonConnector.connect("127.0.0.1:1").await.is_err());
        assert!(RealAsyncConnector.connect("127.0.0.1:1").await.is_err());
    })
    .unwrap();
}

#[test]
fn run_shell_subcommand_drives_the_repl_loop_until_eof() {
    // The dispatch arm constructs a real rustyline editor on the real
    // stdin. Piped stdin (CI, cargo test capture) hits EOF immediately
    // and the loop exits; on a real terminal the shell would rightfully
    // wait for keyboard input, so the test cannot drive it there.
    if std::io::stdin().is_terminal() {
        return;
    }
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (json, async_) = empty_connectors();
        let c = cli(Commands::Shell(ShellArgs {
            app: "demo".to_string(),
        }));
        // Ok (clean EOF exit) or a readline error are both acceptable;
        // the contract is that it returns without hanging.
        let _ = run_with_connectors(c, &json, &async_).await;
    })
    .unwrap();
}
