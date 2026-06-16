use crate::server::server_launch::ServerLaunch;
use crate::server::worker::{WorkerRuntime, WorkerRuntimeParams};
use crate::server::worker_loop_stats::WorkerLoopStats;
use crate::server::worker_mailbox::WorkerMailboxMsg;
use crate::server::worker_ring_loop::{
    WorkerRingLoop, WorkerRingLoopWithRingArgs, drive_future_with_ring,
};
use crossbeam_queue::SegQueue;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_sys::io::worker_ring::{WorkerLocalRing, set_current_worker_ring};
use mudu_sys::sync::{SCondvar, SMutex};
use mudu_utils::notifier::{Notifier, Waiter};
use mudu_utils::task_async::{CurrentThreadTaskRuntime, build_current_thread_runtime};
use std::os::fd::RawFd;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use mudu_sys::SysContext;
use tracing::{debug, trace};

pub(crate) struct RecoveryCoordinator {
    total_workers: usize,
    ready_notifier: Option<Notifier>,
    state: SMutex<RecoveryState>,
    condvar: SCondvar,
}

#[derive(Default)]
struct RecoveryState {
    recovered_workers: usize,
    failed: bool,
}

pub(crate) fn sync_serve_iouring(
    mut cfg: ServerLaunch,
    stop: Waiter,
    ready: Option<Notifier>,
) -> RS<()> {
    if cfg.cfg().worker_count() == 0 {
        return Err(m_error!(EC::ParseErr, "invalid io_uring worker count"));
    }
    let sys = SysContext::iouring();
    let prebound_listener = cfg.take_prebound_listener();
    let conn_id_alloc = Arc::new(AtomicU64::new(1));
    let mailboxes: Vec<_> = (0..cfg.cfg().worker_count())
        .map(|_| Arc::new(SegQueue::<WorkerMailboxMsg>::new()))
        .collect();
    let mailbox_fds: Vec<_> = (0..cfg.cfg().worker_count())
        .map(|_| create_mailbox_event_fd())
        .collect::<RS<Vec<_>>>()?;
    let stop_flag = Arc::new(AtomicBool::new(false));
    let recovery_coordinator = Arc::new(RecoveryCoordinator::new(cfg.cfg().worker_count(), ready));

    let stop_for_notifier = stop.clone();
    let shutdown_mailboxes = mailboxes.clone();
    let shutdown_mailbox_fds = mailbox_fds.clone();
    let notifier_stop_flag = stop_flag.clone();
    let notifier =
        mudu_sys::task::sync::spawn_thread_named("iouring-shutdown-notifier", move || {
            let runtime = build_current_thread_runtime().map_err(|e| {
                m_error!(
                    EC::TokioErr,
                    "create runtime for io_uring shutdown notifier error",
                    e
                )
            })?;
            debug!("iouring shutdown notifier waiting for stop");
            runtime.block_on(stop_for_notifier.wait());
            debug!("iouring shutdown notifier observed stop");
            notifier_stop_flag.store(true, Ordering::Relaxed);
            for (mailbox, fd) in shutdown_mailboxes.into_iter().zip(shutdown_mailbox_fds) {
                mailbox.push(WorkerMailboxMsg::Shutdown);
                notify_mailbox_fd(fd)?;
            }
            Ok(())
        })?;

    let mut handles = Vec::with_capacity(cfg.cfg().worker_count());
    for worker_id in 0..cfg.cfg().worker_count() {
        let worker_port = cfg.cfg().listen_port_for_worker(worker_id)?;
        let listen_addr: std::net::SocketAddr =
            format!("{}:{}", cfg.cfg().listen_ip(), worker_port)
                .parse()
                .map_err(|e| {
                    m_error!(EC::ParseErr, "parse io_uring tcp listen address error", e)
                })?;
        let conn_id_alloc = conn_id_alloc.clone();
        let mailbox = mailboxes[worker_id].clone();
        let all_mailboxes = mailboxes.clone();
        let all_mailbox_fds = mailbox_fds.clone();
        let procedure_runtime = cfg.deps().procedure_runtime_for_worker(worker_id);
        let worker_identity = cfg
            .deps()
            .worker_registry()
            .worker(worker_id)
            .cloned()
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("missing worker identity {}", worker_id)
                )
            })?;
        let worker_registry = cfg.deps().worker_registry();
        let data_dir = cfg.cfg().data_dir().to_string();
        let log_dir = cfg.cfg().log_dir().to_string();
        let log_chunk_size = cfg.cfg().log_chunk_size();
        let log_batching = cfg.deps().log_batching();
        let worker_count = cfg.cfg().worker_count();
        let server_instance_id = cfg.cfg().server_instance_id();
        let listener = match &prebound_listener {
            Some(_) if worker_id != 0 => None,
            Some(listener) => Some(
                listener
                    .try_clone()
                    .map_err(|e| m_error!(EC::NetErr, "clone tcp listener error", e))?,
            ),
            None => None,
        };
        let stop = stop_flag.clone();
        let recovery_coordinator = recovery_coordinator.clone();
        let mailbox_fd = mailbox_fds[worker_id];
        let async_runtime = Some(sys.provider_arc());
        let sys = sys.clone();
        let recovery_coordinator_for_failure = recovery_coordinator.clone();
        let handle =
            mudu_sys::task::sync::spawn_thread_named(format!("worker-{worker_id}"), move || {
                let result = (|| {
                    let runtime = CurrentThreadTaskRuntime::new().map_err(|e| {
                        m_error!(EC::TokioErr, "create runtime for io_uring worker error", e)
                    })?;
                    runtime.block_on(async move {
                        let listener_fd = match listener {
                            Some(std_listener) => std_listener.into_raw_fd(),
                            None => {
                                let rt = sys.provider_arc();
                                let l = rt.net().bind_tcp(listen_addr).await?;
                                let fd = l.as_raw_fd().ok_or_else(|| {
                                    m_error!(
                                        EC::InternalErr,
                                        "async listener does not expose raw fd"
                                    )
                                })?;
                                let dup_fd = unsafe { libc::dup(fd) };
                                if dup_fd < 0 {
                                    return Err(m_error!(
                                        EC::NetErr,
                                        "dup listener fd error",
                                        std::io::Error::last_os_error()
                                    ));
                                }
                                dup_fd
                            }
                        };
                        // Create the worker-local io_uring ring up front so
                        // that worker initialization (meta catalog, WAL tail
                        // scan) can use the io_uring AsyncFs. We drive the ring
                        // while creating the worker, then hand both to the loop.
                        let mut ring = WorkerRingLoop::new_ring()?;
                        #[allow(clippy::arc_with_non_send_sync)]
                        let worker_local_ring =
                            Arc::new(WorkerLocalRing::new_with_task_wake_fd(Some(mailbox_fd)));
                        set_current_worker_ring(worker_local_ring.clone());
                        let worker_fut = WorkerRuntime::new(WorkerRuntimeParams {
                            identity: worker_identity,
                            worker_count,
                            log_dir: log_dir.clone(),
                            data_dir: data_dir.clone(),
                            log_chunk_size,
                            log_batching,
                            procedure_runtime,
                            registry: worker_registry,
                            async_runtime,
                            server_instance_id,
                        });
                        let worker = match drive_future_with_ring(
                            &mut ring,
                            &worker_local_ring,
                            worker_fut,
                            "worker creation",
                        ) {
                            Ok(worker) => worker,
                            Err(e) => {
                                mudu_sys::io::worker_ring::unset_current_worker_ring();
                                return Err(e);
                            }
                        };
                        let mut loop_state =
                            WorkerRingLoop::new_with_ring(WorkerRingLoopWithRingArgs {
                                worker,
                                listener_fd,
                                mailbox_fd,
                                mailbox,
                                mailboxes: all_mailboxes,
                                mailbox_fds: all_mailbox_fds,
                                conn_id_alloc,
                                recovery_coordinator,
                                stop,
                                ring,
                                worker_local_ring,
                            })?;
                        loop_state.initialize().await?;
                        loop_state.run().await
                    })
                })();
                if result.is_err() {
                    recovery_coordinator_for_failure.worker_failed();
                }
                result
            })?;
        handles.push(handle);
    }
    let mut worker_stats = Vec::<WorkerLoopStats>::with_capacity(cfg.cfg().worker_count());

    let mut first_error: Option<mudu::error::err::MError> = None;
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| m_error!(EC::ThreadErr, "join io_uring worker error"))?;
        match result {
            Ok(stats) => {
                worker_stats.push(stats);
            }
            Err(e) => {
                tracing::error!("io_uring worker error, {}", e);
                if first_error.is_none() {
                    first_error = Some(e);
                }
            }
        }
    }

    if first_error.is_none() {
        let notify_result = notifier
            .join()
            .map_err(|_| m_error!(EC::ThreadErr, "join io_uring shutdown notifier error"))?;
        notify_result?;
        log_worker_stats(&worker_stats);
    }
    for fd in mailbox_fds {
        unsafe {
            libc::close(fd);
        }
    }

    if let Some(err) = first_error {
        return Err(m_error!(
            EC::ThreadErr,
            "io_uring backend stopped due to worker error",
            err
        ));
    }
    Ok(())
}

impl RecoveryCoordinator {
    pub(crate) fn new(total_workers: usize, ready_notifier: Option<Notifier>) -> Self {
        Self {
            total_workers,
            ready_notifier,
            state: SMutex::new(RecoveryState::default()),
            condvar: SCondvar::new(),
        }
    }

    pub(crate) fn worker_succeeded(&self) -> RS<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "recovery coordinator lock poisoned"))?;
        if state.failed {
            return Err(m_error!(
                EC::ThreadErr,
                "worker recovery aborted because another worker failed"
            ));
        }
        state.recovered_workers += 1;
        trace!(
            recovered_workers = state.recovered_workers,
            total_workers = self.total_workers,
            "iouring recovery coordinator worker reached barrier"
        );
        if state.recovered_workers == self.total_workers {
            // In io_uring mode the listener can start accepting sockets before
            // every worker has finished WAL recovery. Publish readiness only
            // after the final worker reaches the common recovery barrier so
            // callers do not race listener availability with service
            // availability.
            if let Some(ready_notifier) = &self.ready_notifier {
                trace!("iouring recovery coordinator publishing ready barrier");
                ready_notifier.notify_all();
            }
            self.condvar.notify_all();
            return Ok(());
        }
        // Recovery must be complete on every worker before the service loop
        // starts. If one worker fails recovery, wake everybody and abort
        // instead of leaving the successful workers stuck forever.
        while !state.failed && state.recovered_workers < self.total_workers {
            trace!(
                recovered_workers = state.recovered_workers,
                total_workers = self.total_workers,
                "iouring recovery coordinator waiting for peers"
            );
            state = self.condvar.wait(state).map_err(|err| {
                m_error!(
                    EC::InternalErr,
                    "recovery coordinator condvar wait poisoned",
                    err
                )
            })?;
        }
        if state.failed {
            return Err(m_error!(
                EC::ThreadErr,
                "worker recovery aborted because another worker failed"
            ));
        }
        Ok(())
    }

    pub(crate) fn worker_failed(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.failed = true;
            self.condvar.notify_all();
        }
    }
}

fn create_mailbox_event_fd() -> RS<RawFd> {
    create_event_fd("create io_uring worker mailbox eventfd error")
}

fn create_event_fd(message: &str) -> RS<RawFd> {
    mudu_sys::sync::blocking::eventfd().map_err(|e| m_error!(EC::NetErr, message, e))
}

pub(super) fn notify_mailbox_fd(fd: RawFd) -> RS<()> {
    debug!(fd, "server_iouring notify mailbox fd");
    notify_event_fd(fd, "write io_uring worker mailbox eventfd error")
}

fn notify_event_fd(fd: RawFd, message: &str) -> RS<()> {
    mudu_sys::sync::blocking::notify_eventfd(fd).map_err(|e| m_error!(EC::NetErr, message, e))
}

fn log_worker_stats(stats: &[WorkerLoopStats]) {
    for stat in stats {
        debug!(
            "iouring worker stats: \n\
            worker={}, submit_calls={}, wait_cqe_calls={}, \n\
            accept_submit={}, mailbox_submit={}, recv_submit={}, send_submit={}, \
            log_write_submit={}, cqe_accept={}, cqe_mailbox={}, cqe_recv={}, cqe_send={}, \
            cqe_log_write={}, cqe_close={}, recv_queue_push={}, recv_queue_pop={}, \
            send_queue_push={}, send_queue_pop={}, mailbox_drained={}, local_register={}",
            stat.worker_id,
            stat.submit_calls,
            stat.wait_cqe_calls,
            stat.accept_submit,
            stat.mailbox_submit,
            stat.recv_submit,
            stat.send_submit,
            stat.log_write_submit,
            stat.cqe_accept,
            stat.cqe_mailbox,
            stat.cqe_recv,
            stat.cqe_send,
            stat.cqe_log_write,
            stat.cqe_close,
            stat.recv_queue_push,
            stat.recv_queue_pop,
            stat.send_queue_push,
            stat.send_queue_pop,
            stat.mailbox_drained,
            stat.local_register,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mailbox_eventfd_accumulates_wakeups() {
        let fd = create_mailbox_event_fd().unwrap();
        notify_mailbox_fd(fd).unwrap();
        notify_mailbox_fd(fd).unwrap();

        let value = mudu_sys::sync::blocking::read_eventfd(fd).unwrap();
        assert_eq!(value, 2);

        mudu_sys::sync::blocking::close_fd(fd).unwrap();
    }

    #[test]
    fn mailbox_can_store_shutdown_messages() {
        let mailbox = SegQueue::new();
        mailbox.push(WorkerMailboxMsg::Shutdown);
        assert!(matches!(mailbox.pop(), Some(WorkerMailboxMsg::Shutdown)));
        assert!(mailbox.pop().is_none());
    }
}

pub fn sockaddr_to_socket_addr(
    storage: &mudu_sys::io::iouring::SockAddrBuf,
) -> RS<std::net::SocketAddr> {
    mudu_sys::io::net::sockaddr_to_socket_addr(storage)
}
