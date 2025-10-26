#[cfg(test)]
mod test {
    use crate::contract::a_task::AsyncTask;
    use crate::contract::x_log::{OptAppend, XLog};
    use crate::contract::xl_rec::XLRec;
    use crate::sync::s_mutex::SMutex;
    use crate::x_log::x_log_service::XLogService;
    use crate::x_log::xl_cfg::XLCfg;
    use chrono::{DateTime, Local};
    use mudu::common::result::RS;
    use mudu::error::ec::EC as ER;
    use mudu::error::err::MError;
    use mudu::m_error;
    use mudu_utils::debug;
    use mudu_utils::log::log_setup;
    use mudu_utils::notifier::Notifier;
    use mudu_utils::task::spawn_local_task;
    use mudu_utils::task_trace;
    use std::sync::Arc;
    use std::thread::Builder;
    use std::time;
    use std::time::{Duration, SystemTime};
    use test_utils::_arbitrary::_arbitrary_data;
    use tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender};
    use tokio::task::LocalSet;
    use tracing::info;

    type DataReceiver = UnboundedReceiver<Option<Vec<u8>>>;
    type DataSender = UnboundedSender<Option<Vec<u8>>>;

    fn _fuzz_log_rec(data: &[u8]) -> Vec<XLRec> {
        let r = _arbitrary_data(data);
        r
    }

    //#[test]
    #[allow(dead_code)]
    fn test_uring_log() {
        x_log_run(10, 2, 20, true, None);
    }

    //#[test]
    #[allow(dead_code)]
    fn test_tokio_log() {
        x_log_run(10, 2, 20, false, None);
    }

    fn x_log_run(
        num_entries: usize,
        num_threads: usize,
        num_tasks: usize,
        use_io_uring: bool,
        opt_data: Option<&[u8]>,
    ) {
        log_setup("info");
        let folder = {
            let curr_time = SystemTime::now();
            let dt: DateTime<Local> = curr_time.into();
            format!("/tmp/xl_{}", dt.format("%+"))
        };
        let cfg = XLCfg::new(
            folder,
            "xl".to_string(),
            num_threads as u32,
            1000,
            use_io_uring,
        );
        let canceller = Notifier::new();
        let service = recovery(cfg.clone(), canceller.clone(), Notifier::new());
        let x_log_vec = service.x_log_channel();
        let mut thd_debug = vec![];
        {
            let c = canceller.clone();
            let thd = Builder::new()
                .spawn(move || debug::debug_serve(c, 2024))
                .unwrap();
            thd_debug.push(thd);
        }
        let mut senders = vec![];
        let mut thd_task = vec![];
        for (x_log, param) in x_log_vec {
            let mut receiver = vec![];
            for _n in 0..num_tasks {
                let (s, r) = unbounded_channel();
                receiver.push(r);
                senders.push(s);
            }
            let thd = Builder::new()
                .spawn(move || thread_run(x_log, param, receiver))
                .unwrap();
            thd_task.push(thd);
        }

        let n = num_entries;
        for _i in 0..n {
            for s in senders.iter() {
                let data = match opt_data {
                    None => {
                        let mut v = Vec::new();
                        v.resize(_i % 1000, (_i % 100) as u8);
                        v
                    }
                    Some(data) => data.to_vec(),
                };
                let _ = s.send(Some(data));
            }
        }

        for _i in 0..n {
            for s in senders.iter() {
                let _ = s.send(None);
            }
        }

        let mut duration: Duration = Default::default();
        let mut count = 0;
        for t in thd_task {
            let (d, n) = t.join().unwrap();
            duration += d;
            count += n;
        }

        if duration.as_secs() != 0 {
            let n = (count * num_tasks as u64 * num_threads as u64) / duration.as_secs();
            info!("log write per second: {}", n);
        }
        // done, stop debug service
        canceller.notify_all();

        for t in thd_debug {
            t.join().unwrap();
        }
    }

    fn recovery(folder: XLCfg, canceller: Notifier, recovery_done_notifier: Notifier) -> XLogService {
        let (sender, s) = std::sync::mpsc::channel::<XLogService>();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ls = LocalSet::new();
        let c = canceller.clone();
        let _ = runtime.block_on(async move {
            ls.spawn_local(async move {
                let _r = spawn_local_task(c.clone(), "", async move {
                    let service = async_recovery(folder, c, recovery_done_notifier.clone()).await;
                    sender.send(service).unwrap();
                    Ok::<(), ER>(())
                });
                match _r {
                    Ok(j) => {
                        let _ = j.await;
                    }
                    Err(_e) => {}
                }
            });

            ls.await;
        });
        s.recv().unwrap()
    }

    async fn async_recovery(
        cfg: XLCfg,
        canceller: Notifier,
        recovery_done_notifier: Notifier,
    ) -> XLogService {
        let _trace = task_trace!();
        let service = XLogService::new(cfg, canceller, recovery_done_notifier).unwrap();
        let task = service.recovery_task();
        task.run_once().await.unwrap();
        service
    }

    fn thread_run(
        x_log: Arc<dyn XLog>,
        param: AsyncTask,
        channel: Vec<DataReceiver>,
    ) -> (Duration, u64) {
        let canceler: Notifier = Notifier::new();
        let c = canceler.clone();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ls = LocalSet::new();
        let ret: Arc<SMutex<(Duration, u64)>> = Arc::new(SMutex::new(Default::default()));
        let _ret = ret.clone();
        runtime.block_on(async {
            ls.spawn_local(async move {
                let _r = spawn_local_task(c, "task", async move {
                    let (d, n) = thread_task(canceler, x_log, param, channel).await?;
                    Ok::<_, MError>((d, n))
                });
                match _r {
                    Ok(j) => {
                        let r = j.await;
                        if let Ok(Some(Ok(d))) = r {
                            let mut g = _ret.lock().unwrap();
                            *g = d;
                        }
                    }
                    Err(_e) => {}
                }
                Ok::<_, ER>(())
            });
            ls.await;
        });

        let g = ret.lock().unwrap();
        (*g).clone()
    }

    async fn thread_task(
        canceler: Notifier,
        x_log: Arc<dyn XLog>,
        fsync_task: AsyncTask,
        channel: Vec<DataReceiver>,
    ) -> RS<(Duration, u64)> {
        let c = canceler.clone();
        let mut vec_write_task = vec![];
        let mut ret: (Duration, u64) = Default::default();

        let j = spawn_local_task(canceler.clone(), "", async move {
            let r = write_log_multi_task(c, x_log, channel).await;
            r
        })
            .unwrap();
        vec_write_task.push(j);

        let c = canceler.clone();
        let mut vec_fsync_task = vec![];

        let _j = spawn_local_task(canceler.clone(), "", async move {
            fsync_task.run_once().await.unwrap();
        })
            .unwrap();
        vec_fsync_task.push(_j);
        for j in vec_write_task {
            let opt = j
                .await
                .map_err(|e| m_error!(ER::MuduError, "join write log task error", e))?;
            if let Some(r) = opt {
                let (d, n) = r?;
                ret.0 += d;
                ret.1 += n;
            }
        }
        c.notify_all();
        for j in vec_fsync_task {
            let _ = j.await;
        }
        Ok(ret)
    }

    async fn write_log_multi_task(
        canceler: Notifier,
        x_log: Arc<dyn XLog>,
        channel: Vec<DataReceiver>,
    ) -> RS<(Duration, u64)> {
        let mut vec_j = vec![];
        for (_i, ch) in channel.into_iter().enumerate() {
            let xl = x_log.clone();
            let c = canceler.clone();
            let j = spawn_local_task(canceler.clone(), "write log", async move {
                let r = write_log(c, xl, ch).await;
                r
            })
                .unwrap();
            vec_j.push(j);
        }
        let mut duration = Duration::default();
        let mut count = 0;
        for j in vec_j {
            let r_join = j
                .await
                .map_err(|e| m_error!(ER::MuduError, "join error", e))?;
            if let Some(r) = r_join {
                let (d, n) = r?;
                duration += d;
                count += n;
            }
        }

        Ok((duration, count))
    }

    async fn write_log(
        _canceler: Notifier,
        x_log: Arc<dyn XLog>,
        channel: DataReceiver,
    ) -> RS<(Duration, u64)> {
        let _trace = task_trace!();
        let _r = _write_log(x_log, channel).await;
        match _r {
            Ok(d) => Ok(d),
            Err(e) => {
                println!("{:?}", e);
                Err(e)
            }
        }
    }


    async fn _write_log(x_log: Arc<dyn XLog>, channel: DataReceiver) -> RS<(Duration, u64)> {
        let _trace = task_trace!();
        let mut ch = channel;
        let mut opt_max_lsn = None;
        let mut n = 0;
        let mut duration = Duration::from_millis(0);
        loop {
            let opt = ch.recv().await;
            let log_rec: Vec<XLRec> = match opt {
                None => {
                    break;
                }
                Some(opt_data) => {
                    if let Some(data) = opt_data {
                        _arbitrary_data(&data)
                    } else {
                        break;
                    }
                }
            };
            let inst = time::Instant::now();
            let (lsn, _) = x_log.append(log_rec, OptAppend::default()).await?;
            let max_lsn = lsn;
            if max_lsn % 10 == 9 {
                let f = x_log.flush(max_lsn).await?;
                //info!("wait log {}", max_lsn);
                f.wait().await?;
                //info!("flush log {}", max_lsn);
            }
            if let Some(lsn) = &mut opt_max_lsn {
                *lsn = max_lsn
            }
            duration += inst.elapsed();
            n += 1;
        }
        if let Some(lsn) = opt_max_lsn {
            let inst = time::Instant::now();
            let f = x_log.flush(lsn).await?;
            f.wait().await?;
            //info!("flush log {}", lsn);
            duration += inst.elapsed();
        }

        Ok((duration, n))
    }


    pub fn _x_log_append(data: &[u8]) {
        x_log_run(1, 10, 10, false, Some(data));
    }


    #[cfg(test)]
    mod _test {
        use crate::fuzz::_test_target::_test::_test_target;

        //#[test]
        fn _x_log_append() {
            _test_target("_x_log_append");
        }
    }
}