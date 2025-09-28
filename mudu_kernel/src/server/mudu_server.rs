use mudu_utils::notifier::Notifier;

use crate::common::mudu_cfg::{load_mudu_conf, MuduCfg};
use crate::contract::a_task::{ATaskRef, AsyncTask};
use crate::contract::s_task::{STask, SyncTask};
use crate::meta::meta_mgr_factory::MetaMgrFactory;
use crate::server::accept_handle_task::AcceptHandleTask;
use crate::server::incoming_session::IncomingSession;
use crate::server::session_handle_task::SessionHandleTask;
use crate::storage::mem_store_factory::MemStoreFactory;
use crate::storage::pst_store_factory::PstStoreFactory;
use crate::sync::notify_wait::{create_notify_wait, Notify, Wait};
use crate::sync::unique_inner::UniqueInner;
use crate::tx::tx_mgr_factory::TxMgrFactory;
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_log::x_log_service::XLogService;
use crate::x_log::xl_cfg::XLCfg;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::debug;
use mudu_utils::task::spawn_local_task;
use std::net::{IpAddr, SocketAddr};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use tokio::runtime;
use tokio::sync::mpsc::channel;
use tokio::task::LocalSet;
use tracing::{error, info};

pub struct MuduServer {
    cfg: MuduCfg,
    thread_handle: Vec<JoinHandle<RS<()>>>,
    stop_notify: Notify<()>,
}

pub struct MuduStop {
    canceller: Notifier,
    wait: Wait<()>,
}

struct DebugServer {
    canceler: Notifier,
    port: u16,
}

impl MuduStop {
    fn new(canceller: Notifier, wait: Wait<()>) -> MuduStop {
        Self { canceller, wait }
    }
    pub fn stop(&self) {
        let _ = self.canceller.notify_all();
        let wait = self.wait.clone();
        let runtime = runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async move {
            let _ = wait.wait().await;
        });
    }
}

impl MuduServer {
    pub fn start(cfg_path: Option<String>) -> RS<(Self, MuduStop)> {
        let mut thread_handle = vec![];
        let canceller = Notifier::new();

        let cfg = load_mudu_conf(cfg_path)?;

        let wait_recovery = Notifier::new();
        let x_log_path = PathBuf::from(&cfg.db_path);
        let x_log_path = x_log_path.join(&cfg.x_log_folder);
        let x_log_path = x_log_path.to_str().unwrap().to_string();
        let xl_conf = XLCfg {
            x_log_use_io_uring: cfg.x_log_use_io_uring,
            x_log_path,
            x_log_ext_name: cfg.x_log_ext_name.clone(),
            x_log_channels: cfg.x_log_channels,
            x_log_file_size_limit: cfg.x_log_file_size_limit,
        };
        let x_log_service = XLogService::new(xl_conf, canceller.clone(), wait_recovery.clone())?;
        let x_log_and_fsync_task = x_log_service.x_log_channel();
        let tree_store = MemStoreFactory::create(cfg.db_path.clone())?;
        let meta_mgr = MetaMgrFactory::create(cfg.db_path.clone())?;
        let x_lock_mgr = TxMgrFactory::create_lock_mgr();
        let x_snap_mgr = TxMgrFactory::create_snap_mgr(canceller.clone(), 0, 100);
        let (kv_sync_task, ch) = PstStoreFactory::create(cfg.db_path.clone())?;
        if cfg.session_threads as usize > x_log_and_fsync_task.len() {
            panic!("log channel size must larger than threads")
        }

        {
            let debug_server = DebugServer::new(canceller.clone(), 1800);
            let debug_server_task = Arc::new(UniqueInner::new(debug_server));
            let j = Self::create_sync_thread("debug_serve".to_string(), debug_server_task)?;
            thread_handle.push(j);
        }

        let mut session_receiver = vec![];
        let mut session_sender = vec![];

        for _i in 0..cfg.session_threads {
            let (s, r) = channel::<IncomingSession>(10);
            session_receiver.push(r);
            session_sender.push(s);
        }

        {
            // async recovery task
            let task = x_log_service.recovery_task();
            let j = Self::create_thread(canceller.clone(), "recovery".to_string(), vec![task])?;
            thread_handle.push(j);
        }

        {
            // snapshot assignment task
            let task = x_snap_mgr.snap_assign_task();
            let j = Self::create_thread(canceller.clone(), "snap_assign".to_string(), vec![task])?;
            thread_handle.push(j);
        }

        {
            // main accept connected session task
            let ip_addr = IpAddr::from_str(cfg.server_bind_address.as_str())
                .map_err(|e| m_error!(ER::ParseErr, "ip address parse error", e))?;
            let bind_addr = SocketAddr::new(ip_addr, cfg.server_listen_port);
            let accept_handle_task =
                AcceptHandleTask::new(canceller.clone(), bind_addr, session_sender, wait_recovery);
            let task: Arc<dyn ATaskRef> = Arc::new(UniqueInner::new(accept_handle_task));
            let j = Self::create_thread(canceller.clone(), "accept".to_string(), vec![task])?;
            thread_handle.push(j);
        }
        {
            let j = Self::create_sync_thread("kv_flush".to_string(), kv_sync_task)?;
            thread_handle.push(j);
        }
        {
            // session handle tasks
            let mut logs: Vec<Vec<_>> = vec![];
            let mut tasks: Vec<Vec<_>> = vec![];
            logs.resize(cfg.session_threads as usize, vec![]);
            tasks.resize(cfg.session_threads as usize, vec![]);
            for (i, (log, task)) in x_log_and_fsync_task.into_iter().enumerate() {
                let n = i % cfg.session_threads as usize;
                logs[n].push(log);
                tasks[n].push(task);
            }

            let mut session_receiver: Vec<_> = session_receiver.into_iter().rev().collect();
            let receiver_size = session_receiver.len();
            for i in 0..cfg.session_threads {
                let n = i as usize;
                let logs = logs[n].clone();
                let mut tasks = tasks[n].clone();
                let opt = session_receiver.pop();
                let receiver = match opt {
                    Some(h) => h,
                    None => {
                        panic!(
                            "session_receiver size {}, is not equal to the thread count {}, \
                    which should be expected equal",
                            receiver_size, cfg.session_threads
                        )
                    }
                };
                let thd_ctx = ThdCtx::new(
                    n as u64,
                    meta_mgr.clone(),
                    Arc::new(x_snap_mgr.snapshot_requester()),
                    x_lock_mgr.clone(),
                    logs,
                    tree_store.clone(),
                    ch.clone(),
                );
                let session_handler = SessionHandleTask::new(
                    thd_ctx,
                    format!("session_handler_{}", n),
                    receiver,
                    canceller.clone(),
                );
                tasks.push(Arc::new(UniqueInner::new(session_handler)));

                let j =
                    Self::create_thread(canceller.clone(), format!("session_{}", i + 1), tasks)?;
                thread_handle.push(j);
            }
        }
        let (notify, wait) = create_notify_wait();
        let server = Self {
            cfg,
            thread_handle,
            stop_notify: notify,
        };
        let stop = MuduStop::new(canceller, wait);
        Ok((server, stop))
    }

    pub fn join(self) -> RS<()> {
        for t in self.thread_handle {
            let r = t.join();
            match r {
                Err(e) => {
                    error!("join error {:?}", e)
                }
                Ok(rr) => match rr {
                    Err(e) => {
                        error!("thread error {:?}", e)
                    }
                    Ok(_) => {}
                },
            }
        }
        Ok(())
    }

    fn create_thread(
        canceller: Notifier,
        name: String,
        tasks: Vec<AsyncTask>,
    ) -> RS<JoinHandle<RS<()>>> {
        let r_thd = thread::Builder::new()
            .name(name.clone())
            .spawn(move || Self::_thread_task(canceller, name, tasks));
        let thd = r_thd.map_err(|e|
            m_error!(ER::ThreadErr, "create thread error", e))?;
        Ok(thd)
    }

    fn create_sync_thread(name: String, task: SyncTask) -> RS<JoinHandle<RS<()>>> {
        let r_thd = thread::Builder::new()
            .name(name.clone())
            .spawn(move || task.run_once());
        let thd = r_thd.map_err(|e|
            m_error!(ER::ThreadErr, "spawn new thread error", e))?;
        Ok(thd)
    }

    fn _thread_task(canceller: Notifier, name: String, tasks: Vec<AsyncTask>) -> RS<()> {
        let r_tokio_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build();
        let runtime = r_tokio_runtime.map_err(|e|
            m_error!(ER::TokioErr, "tokio error", e)
        )?;
        let ls = LocalSet::new();

        for task in tasks {
            let c = canceller.clone();
            let n = name.clone();
            ls.spawn_local(async move {
                spawn_local_task(c, "", async move {
                    let r = task.run_once().await;
                    match r {
                        Ok(()) => {}
                        Err(e) => {
                            error!("session thread {} run task error {:?}", n, e);
                        }
                    }
                })
            });
        }

        runtime.block_on(ls);
        info!("task {} done", name);
        Ok(())
    }
}

impl DebugServer {
    fn new(canceler: Notifier, port: u16) -> Self {
        Self { canceler, port }
    }
}

impl STask for DebugServer {
    fn name(&self) -> String {
        "debug server".to_owned()
    }

    fn run(self) -> RS<()> {
        debug::debug_serve(self.canceler, self.port);
        Ok(())
    }
}
