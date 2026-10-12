#![allow(clippy::await_holding_lock)]

use super::*;

// Miri cannot execute the io_uring syscalls used by this performance test.
#[test]
#[cfg_attr(miri, ignore)]
fn iouring_backend_perf_put_get() -> RS<()> {
    mudu_sys::task::async_::build_multi_thread_runtime()
        .unwrap()
        .block_on(async move {
            if maybe_skip_in_act() {
                return Ok(());
            }
            let _guard = lock_network_perf_test();
            log_setup("info");
            let notifier = NotifyWait::new();
            {
                let _n = notifier.clone();
                let _ = mudu_sys::task::sync::spawn_thread(move || {
                    debug_serve(_n, 1800);
                })?;
            };
            let worker_count = 6usize;
            let clients = 6usize;
            let Some(listeners) = reserve_listener_block(worker_count) else {
                return Ok(());
            };
            let port = listeners[0].local_addr().unwrap().port();
            let bench_duration = Duration::from_secs(10);
            let data_dir = temp_dir().join(format!(
                "mududb_iouring_perf_{}",
                mudu_sys::random::uuid_v4()
            ));
            mudu_sys::fs::sync::create_dir_all(&data_dir).unwrap();

            let (stop_notifier, server_thread) =
                spawn_iouring_server(listeners, worker_count, &data_dir, 64 * 1024 * 1024, None);
            if wait_ready_or_skip_io_uring(port, &server_thread, "skip io_uring perf test").await? {
                return Ok(());
            }

            run_put_get_bench(port, clients, bench_duration, &notifier).await?;

            stop_notifier.notify_all();
            server_thread.join().unwrap()?;
            Ok(())
        })
}

/// Shared latches and counters of the put/get benchmark clients.
struct PutGetCounters {
    start_clients: Arc<AtomicBool>,
    start_notify: Arc<Notify>,
    ready_clients: Arc<AtomicU64>,
    setup_error: Arc<SMutex<Option<String>>>,
    stop_clients: Arc<AtomicBool>,
    put_ops: Arc<AtomicU64>,
    get_ops: Arc<AtomicU64>,
    put_latencies_us: Arc<SMutex<Vec<u64>>>,
    get_latencies_us: Arc<SMutex<Vec<u64>>>,
}

impl Default for PutGetCounters {
    fn default() -> Self {
        Self {
            start_clients: Arc::new(AtomicBool::new(false)),
            start_notify: Arc::new(Notify::new()),
            ready_clients: Arc::new(AtomicU64::new(0)),
            setup_error: Arc::new(SMutex::new(None::<String>)),
            stop_clients: Arc::new(AtomicBool::new(false)),
            put_ops: Arc::new(AtomicU64::new(0)),
            get_ops: Arc::new(AtomicU64::new(0)),
            put_latencies_us: Arc::new(SMutex::new(Vec::<u64>::new())),
            get_latencies_us: Arc::new(SMutex::new(Vec::<u64>::new())),
        }
    }
}

/// Drives the measurement: spawns the clients, releases them simultaneously,
/// stops them after `bench_duration` and reports the aggregated stats.
async fn run_put_get_bench(
    port: u16,
    clients: usize,
    bench_duration: Duration,
    notifier: &NotifyWait,
) -> RS<()> {
    let counters = PutGetCounters::default();
    let mut join_set = spawn_put_get_clients(port, clients, notifier, &counters)?;

    wait_for_clients_ready(
        clients,
        counters.ready_clients.as_ref(),
        counters.setup_error.as_ref(),
    )
    .await?;
    counters.start_clients.store(true, Ordering::Release);
    counters.start_notify.notify_waiters();

    let started_at = mudu_sys::time::instant_now();
    mudu_sys::task::async_::sleep(bench_duration).await?;
    let elapsed = started_at.elapsed();
    counters.stop_clients.store(true, Ordering::Relaxed);
    while let Some(result) = join_set.join_next().await {
        result.map_err(|e| {
            mudu_error!(
                ErrorCode::Tokio,
                "join io_uring perf client aggregation task error",
                e
            )
        })??;
    }
    report_put_get_stats(clients, elapsed, &counters);
    Ok(())
}

/// Spawns one benchmark client task per client id, each looping put/get over
/// its own key range until `stop_clients` is set.
fn spawn_put_get_clients(
    port: u16,
    clients: usize,
    notifier: &NotifyWait,
    counters: &PutGetCounters,
) -> RS<JoinSet<RS<()>>> {
    let mut join_set: JoinSet<RS<()>> = mudu_sys::tokio::task::JoinSet::new();
    for client_id in 0..clients {
        let start_clients = counters.start_clients.clone();
        let start_notify = counters.start_notify.clone();
        let ready_clients = counters.ready_clients.clone();
        let setup_error = counters.setup_error.clone();
        let stop_clients = counters.stop_clients.clone();
        let put_ops = counters.put_ops.clone();
        let get_ops = counters.get_ops.clone();
        let put_latencies_us = counters.put_latencies_us.clone();
        let get_latencies_us = counters.get_latencies_us.clone();
        let join_handle = spawn_task(
            notifier.as_waiter(),
            format!("task_cli_{}", client_id).as_str(),
            async move {
                let mut client =
                    match AsyncPerfClient::connect_with_loopback_shard(port, client_id).await {
                        Ok(client) => client,
                        Err(err) => {
                            let mut setup_error = setup_error.lock().unwrap();
                            if setup_error.is_none() {
                                *setup_error =
                                    Some(format!("client {client_id} connect/setup error: {err}"));
                            }
                            return Err(err);
                        }
                    };
                ready_clients.fetch_add(1, Ordering::AcqRel);
                while !start_clients.load(Ordering::Acquire) {
                    start_notify.notified().await;
                }
                let mut op_id = 0usize;
                let mut local_put_latencies_us = Vec::new();
                let mut local_get_latencies_us = Vec::new();
                while !stop_clients.load(Ordering::Relaxed) {
                    let key = format!("client-{client_id:02}-key-{op_id:06}").into_bytes();
                    let value = format!("value-{client_id:02}-{op_id:06}").into_bytes();
                    debug!("client {} put key ", client_id);
                    let put_started_at = mudu_sys::time::instant_now();
                    client.put(key.clone(), value.clone()).await?;
                    local_put_latencies_us.push(put_started_at.elapsed().as_micros() as u64);
                    debug!("client {} put key done", client_id);
                    put_ops.fetch_add(1, Ordering::Relaxed);
                    debug!("client {} get key", client_id);
                    let get_started_at = mudu_sys::time::instant_now();
                    let returned = client.get(key).await?;
                    local_get_latencies_us.push(get_started_at.elapsed().as_micros() as u64);
                    debug!("client {} get key done", client_id);
                    assert_eq!(returned, Some(value));
                    get_ops.fetch_add(1, Ordering::Relaxed);
                    op_id += 1;
                }
                put_latencies_us
                    .lock()
                    .unwrap()
                    .extend(local_put_latencies_us);
                get_latencies_us
                    .lock()
                    .unwrap()
                    .extend(local_get_latencies_us);
                Ok::<(), MuduError>(())
            },
        )?;
        join_set.spawn(async move {
            match join_handle.await {
                Ok(Some(Ok(()))) => Ok(()),
                Ok(Some(Err(err))) => Err(err),
                Ok(None) => Err(mudu_error!(
                    ErrorCode::Tokio,
                    format!("io_uring perf client task {} cancelled", client_id)
                )),
                Err(err) => Err(mudu_error!(
                    ErrorCode::Tokio,
                    format!("join io_uring perf client task {} error", client_id),
                    err
                )),
            }
        });
    }
    Ok(join_set)
}

/// Computes the throughput and latency percentiles and logs them.
fn report_put_get_stats(clients: usize, elapsed: Duration, counters: &PutGetCounters) {
    let total_put_ops = counters.put_ops.load(Ordering::Relaxed) as usize;
    let total_get_ops = counters.get_ops.load(Ordering::Relaxed) as usize;
    let total_ops = total_put_ops + total_get_ops;
    let throughput = total_ops as f64 / elapsed.as_secs_f64();
    let mut put_samples = counters.put_latencies_us.lock().unwrap().clone();
    let mut get_samples = counters.get_latencies_us.lock().unwrap().clone();
    let mut total_samples = Vec::with_capacity(put_samples.len() + get_samples.len());
    total_samples.extend_from_slice(&put_samples);
    total_samples.extend_from_slice(&get_samples);
    let put_p1_us = percentile_us(&mut put_samples, 0.01);
    let put_p50_us = percentile_us(&mut put_samples, 0.50);
    let put_p99_us = percentile_us(&mut put_samples, 0.99);
    let put_p999_us = percentile_us(&mut put_samples, 0.999);
    let get_p1_us = percentile_us(&mut get_samples, 0.01);
    let get_p50_us = percentile_us(&mut get_samples, 0.50);
    let get_p99_us = percentile_us(&mut get_samples, 0.99);
    let get_p999_us = percentile_us(&mut get_samples, 0.999);
    let total_p1_us = percentile_us(&mut total_samples, 0.01);
    let total_p50_us = percentile_us(&mut total_samples, 0.50);
    let total_p99_us = percentile_us(&mut total_samples, 0.99);
    let total_p999_us = percentile_us(&mut total_samples, 0.999);
    let put_avg_us = avg_us(&put_samples);
    let get_avg_us = avg_us(&get_samples);
    let total_avg_us = avg_us(&total_samples);
    info!(
        "io_uring kv perf: clients={}, puts={}, gets={}, total_ops={}, elapsed_ms={}, throughput_ops_per_sec={:.2}, put_avg_us={:.2}, put_p1_us={}, put_p50_us={}, put_tail_p99_us={}, put_tail_p999_us={}, get_avg_us={:.2}, get_p1_us={}, get_p50_us={}, get_tail_p99_us={}, get_tail_p999_us={}, total_avg_us={:.2}, total_p1_us={}, total_p50_us={}, total_tail_p99_us={}, total_tail_p999_us={}",
        clients,
        total_put_ops,
        total_get_ops,
        total_ops,
        elapsed.as_millis(),
        throughput,
        put_avg_us.unwrap_or_default(),
        put_p1_us.unwrap_or_default(),
        put_p50_us.unwrap_or_default(),
        put_p99_us.unwrap_or_default(),
        put_p999_us.unwrap_or_default(),
        get_avg_us.unwrap_or_default(),
        get_p1_us.unwrap_or_default(),
        get_p50_us.unwrap_or_default(),
        get_p99_us.unwrap_or_default(),
        get_p999_us.unwrap_or_default(),
        total_avg_us.unwrap_or_default(),
        total_p1_us.unwrap_or_default(),
        total_p50_us.unwrap_or_default(),
        total_p99_us.unwrap_or_default(),
        total_p999_us.unwrap_or_default()
    );
}
