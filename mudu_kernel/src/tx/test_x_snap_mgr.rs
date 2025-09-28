#[cfg(test)]
mod _test {
    use crate::tx::x_snap_mgr::{SnapshotRequester, XSnapMgr};
    use mudu_utils::notifier::Notifier;

    use mudu_utils::log::log_setup;
    use mudu_utils::task::spawn_local_task;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};
    use tokio::runtime::Builder;
    use tokio::task::LocalSet;
    use tracing::info;

    #[test]
    fn _test_x_snap_mgr() {
        log_setup("info");
        let canceller = Notifier::new();
        let x_snap_mgr = XSnapMgr::new(canceller.clone(), 100, 10);
        let handler = x_snap_mgr.snap_assign_task();
        let c = canceller.clone();
        let thread = std::thread::spawn(move || {
            let ls = LocalSet::new();
            ls.spawn_local(async move {
                let r = handler.run_once().await;
                assert!(r.is_ok())
            });
            let runtime = Builder::new_current_thread().enable_all().build().unwrap();
            runtime.block_on(ls);
        });

        let requester = x_snap_mgr.snapshot_requester();
        run_request(requester, 1000, 10, 4);
        let _ = canceller.notify_all();
        thread.join().unwrap();
    }

    fn run_request(request: SnapshotRequester, num_x: usize, num_task: usize, num_threads: usize) {
        let mut threads = vec![];
        let duration = Arc::new(Mutex::new(Duration::new(0, 0)));
        for i in 0..num_threads {
            let r = request.clone();
            let d = duration.clone();
            let thd = std::thread::spawn(move || {
                thd_task(r, num_x, num_task, d);
            });
            threads.push(thd);
        }
        for t in threads {
            t.join().unwrap();
        }
        {
            let duration = duration.lock().unwrap();
            let total_requests = num_x * num_task * num_threads;
            let n = *duration / total_requests as u32;
            info!(
                "total_request {}, avg request latency: {} millis",
                total_requests,
                n.as_millis()
            )
        }
    }

    fn thd_task(
        request: SnapshotRequester,
        n: usize,
        num_task: usize,
        duration: Arc<Mutex<Duration>>,
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ls = LocalSet::new();
        ls.spawn_local(async move {
            let d = async_task(request, n, num_task).await;
            let mut g_d = duration.lock().unwrap();
            *g_d += d;
        });
        runtime.block_on(async move {
            ls.await;
        })
    }

    async fn async_task(request: SnapshotRequester, n: usize, num_tasks: usize) -> Duration {
        let mut task = vec![];
        for _i in 0..num_tasks {
            let r = request.clone();
            let t = spawn_local_task(Notifier::new(), "", async move {
                let duration = async_request(r, n).await;
                duration
            });
            task.push(t);
        }
        let mut duration = Duration::new(0, 0);
        for t in task {
            match t {
                Ok(j) => {
                    let opt_d = j.await.unwrap();
                    match opt_d {
                        Some(d) => duration += d,
                        None => {
                            panic!("")
                        }
                    }
                }
                _ => {
                    panic!()
                }
            }
        }
        duration
    }

    async fn async_request(requester: SnapshotRequester, n: usize) -> Duration {
        let mut xids = vec![];
        let mut duration = Duration::new(0, 0);
        for _i in 0..n {
            let start = Instant::now();
            let snapshot = requester.start_tx().await;
            duration += start.elapsed();
            xids.push(snapshot.unwrap().xid());
            if xids.len() > 10 {
                for x in xids.iter() {
                    requester.end_tx(*x).await.unwrap();
                }
                xids.clear();
            }
        }
        for x in xids.iter() {
            requester.end_tx(*x).await.unwrap();
        }
        xids.clear();
        duration
    }
}
