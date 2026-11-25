//! Opt-in queue churn baseline, including relation data and physical WAL growth.
use super::*;

fn storage_bytes(path: &std::path::Path) -> RS<(u64, u64)> {
    let mut data = 0;
    let mut wal = 0;
    for path in mudu_sys::fs::sync::read_dir(path)? {
        let metadata = mudu_sys::fs::sync::metadata(&path)?;
        if metadata.is_dir() {
            let (d, w) = storage_bytes(&path)?;
            data += d;
            wal += w;
        } else if path
            .components()
            .any(|part| part.as_os_str() == "relation_wal")
        {
            wal += metadata.len();
        } else if path.extension().is_some_and(|extension| extension == "dat") {
            data += metadata.len();
        }
    }
    Ok((data, wal))
}

#[test]
#[ignore = "queue performance/storage baseline; run explicitly with --ignored --nocapture"]
fn queue_churn_baseline() -> RS<()> {
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test domain lock poisoned"))?;
    let Some(ctx) = QueueTestContext::new()? else {
        return Ok(());
    };
    let server = ctx.start_server()?;
    let (mut client, session) = ctx.connect()?;
    let app = format!("queuechurn_{}", mudu_sys::random::uuid_v4());
    client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
    let mut enqueue_us = Vec::new();
    let mut samples = Vec::new();
    let started = instant_now();
    for index in 0..2000 {
        let before = instant_now();
        client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('payload');")?;
        enqueue_us.push(before.elapsed().as_micros() as u64);
        let response = client.query_with_oid(session, &app, "RECEIVE 32 FROM q WAIT FOREVER;")?;
        assert_eq!(response.rows().len(), 1);
        let seq = envelope_seq(&response.rows()[0]);
        assert_eq!(seq, index);
        client.execute_with_oid(session, &app, format!("ACK q TO {seq};"))?;
        if (index + 1) % 500 == 0 {
            let (data_bytes, relation_wal_bytes) = storage_bytes(&ctx.base_dir)?;
            samples.push(serde_json::json!({"cycles": index + 1, "data_bytes": data_bytes, "relation_wal_bytes": relation_wal_bytes}));
        }
    }
    enqueue_us.sort_unstable();
    println!(
        "QUEUE_BASELINE {}",
        serde_json::json!({
            "cycles": 2000, "elapsed_ms": started.elapsed().as_millis(),
            "enqueue_p50_us": enqueue_us[1000], "enqueue_p99_us": enqueue_us[1980],
            "samples": samples
        })
    );
    assert_eq!(view_u64(&queue_view(server.http_port, "q")?, "depth"), 0);
    Ok(())
}
