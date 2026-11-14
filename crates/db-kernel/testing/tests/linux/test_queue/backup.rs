//! Physical backup restores queue rows, fan-out cursors, and empty watermarks.
use super::*;
use ::backup::restore::{RestoreTarget, restore_backup};
use mudu_runtime::backend::mudud_cfg::BackupCfg;

#[test]
#[cfg_attr(miri, ignore)]
fn queue_base_backup_restores_cursors_and_empty_watermark() -> RS<()> {
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test domain lock poisoned"))?;
    let (Some(src), Some(dst)) = (QueueTestContext::new()?, QueueTestContext::new()?) else {
        return Ok(());
    };
    let backup_dir = src.base_dir.join("backups").to_string_lossy().into_owned();
    let archive_dir = src.base_dir.join("archive").to_string_lossy().into_owned();
    create_dir_all(&archive_dir)?;
    let server = src.start_configured_server(
        65536,
        64 * 1024 * 1024,
        Some(BackupCfg {
            backup_dir: Some(backup_dir.clone()),
            archive_dir: None,
            archive_mode: "off".into(),
        }),
    )?;
    let (mut client, session) = src.connect()?;
    let app = "queue_backup";
    for sql in [
        "CREATE QUEUE empty_q (body TEXT);",
        "INSERT INTO empty_q VALUES ('drained');",
        "CREATE QUEUE fan (body TEXT);",
        "CREATE CONSUMER fast ON fan;",
        "CREATE CONSUMER slow ON fan;",
        "INSERT INTO fan VALUES ('first'), ('second');",
        "ACK fan FOR fast TO 0;",
    ] {
        client.execute_with_oid(session, app, sql)?;
    }
    client.query_with_oid(session, app, "RECEIVE 1 FROM empty_q;")?;
    client.execute_with_oid(session, app, "ACK empty_q TO 0;")?;
    let http_addr = format!("127.0.0.1:{}", server.http_port);
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        mudu_client::management::backup_create(&http_addr, "queue-base").await
    })?
    .map_err(|e| mudu::mudu_error!(ErrorCode::Network, e))?;
    // Writes after the backup must not appear in a base-only restore.
    client.execute_with_oid(session, app, "INSERT INTO fan VALUES ('later');")?;
    drop(client);
    drop(server);
    // Opening relations requires a LocalSet (or worker ring) on the calling
    // thread: run the restore inside one.
    let dst_data = dst.base_dir.join("data").to_string_lossy().into_owned();
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        restore_backup(
            &backup_dir,
            &archive_dir,
            "queue-base",
            &dst_data,
            RestoreTarget::End,
        )
        .await
    })??;
    let restored = dst.start_server()?;
    let (mut client, session) = dst.connect()?;
    let fast = client.query_with_oid(session, app, "RECEIVE 10 FROM fan FOR fast;")?;
    assert_eq!(fast.rows().len(), 1);
    assert_eq!(envelope_seq(&fast.rows()[0]), 1);
    let slow = client.query_with_oid(session, app, "RECEIVE 10 FROM fan FOR slow;")?;
    assert_eq!(slow.rows().len(), 2);
    client.execute_with_oid(session, app, "INSERT INTO empty_q VALUES ('new');")?;
    let row = client.query_with_oid(session, app, "RECEIVE 1 FROM empty_q;")?;
    assert_eq!(envelope_seq(&row.rows()[0]), 1);
    assert_eq!(
        view_u64(&queue_view(restored.http_port, "fan")?, "depth"),
        2
    );
    Ok(())
}
