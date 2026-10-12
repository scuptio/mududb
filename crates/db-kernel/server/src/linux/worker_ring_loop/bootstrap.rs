use super::*;

impl WorkerRingLoop {
    pub async fn initialize(&mut self) -> RS<()> {
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop run start"
        );
        set_current_worker_local(as_worker_local_ref(new_session_bound_worker_runtime(
            self.worker.clone(),
            0,
        )));
        set_current_message_bus(self.message_bus.as_ref());
        // Install the worker-local ring before any initialization that may use
        // the io_uring-based async file system (e.g. meta catalog open), and
        // drive it while initialization runs so queued io_uring I/O completes.
        set_current_worker_ring(self.worker_local_ring.clone());
        //self.worker.meta_mgr().initialize().await?;
        register_worker_message_bus(
            self.worker.server_instance_id(),
            self.worker.worker_id(),
            &self.message_bus.as_ref(),
        )?;
        self.worker.ensure_partition_rpc_handler()?;
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop partition rpc ready"
        );
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop bootstrap storage start"
        );
        let worker = self.worker.clone();
        if let Err(e) = self.drive_local_future(worker.initialize(), "worker initialize") {
            unset_current_worker_ring();
            return Err(e);
        }
        Ok(())
    }
}

impl WorkerRingLoop {
    /// Runs worker recovery and then enters the main service loop.
    ///
    /// The worker-local ring pointer is installed for the duration of the run
    /// so user-level async file I/O can enqueue requests onto this loop.
    pub(crate) async fn run(&mut self) -> RS<WorkerLoopStats> {
        scoped_task_trace!();
        set_current_worker_ring(self.worker_local_ring.clone());
        self.drive_bootstrap_storage()?;

        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop bootstrap storage done"
        );
        // The worker log backend is initialized lazily during initialize().
        // Refresh the ring loop's local log wrapper so recovery and steady-
        // state flush polling use the real backend instead of the placeholder
        // captured in WorkerRingLoop::new before initialization.
        self.refresh_log_wrapper()?;
        if let Err(err) = self.recover_worker_log_on_loop() {
            self.fail_bootstrap();
            return Err(err);
        }
        self.recover_fs_gc_on_loop()?;
        self.recovery_coordinator.worker_succeeded()?;
        trace!(
            worker_id = self.worker.worker_id(),
            "worker_ring_loop recovery barrier passed"
        );
        let worker = self.worker.clone();
        self.spawn(None, async move {
            worker.recover_cross_partition_transactions().await
        });
        let r = self.run_service_loop();
        self.teardown_worker_registration();
        r
    }

    /// Drives `bootstrap_storage_async` to completion on the worker ring,
    /// pumping io_uring submits and completions between polls. Any failure
    /// tears down the worker registration and fails the recovery barrier.
    fn drive_bootstrap_storage(&mut self) -> RS<()> {
        let worker = self.worker.clone();
        let bootstrap_fut = worker.bootstrap_storage_async();
        let mut bootstrap_fut = std::pin::pin!(bootstrap_fut);
        let waker = noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match bootstrap_fut.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(Ok(())) => return Ok(()),
                std::task::Poll::Ready(Err(e)) => {
                    self.fail_bootstrap();
                    return Err(e);
                }
                std::task::Poll::Pending => {
                    if let Err(e) = self.pump_ring_once("bootstrap") {
                        self.fail_bootstrap();
                        return Err(e);
                    }
                }
            }
        }
    }

    /// Rebuilds the ring loop's local log wrapper from the worker's freshly
    /// initialized log backend.
    fn refresh_log_wrapper(&mut self) -> RS<()> {
        self.log = self.worker.worker_log()?.map(|backend| {
            new_xl_batch_worker_log(
                backend.clone(),
                recovery::WorkerRingLoopRecoveryHandler {
                    worker: self.worker.clone(),
                },
            )
        });
        Ok(())
    }

    /// fs GC startup recovery: reclaim generations orphaned by a crash or
    /// an aborted write before the worker starts serving.
    fn recover_fs_gc_on_loop(&mut self) -> RS<()> {
        let worker = self.worker.clone();
        if let Err(err) = self.drive_local_future(worker.fs_gc_recover_scan(), "fs gc recover scan")
        {
            self.fail_bootstrap();
            return Err(err);
        }
        Ok(())
    }

    /// Unregisters the worker's message bus and clears the worker
    /// thread-locals installed for the run.
    fn teardown_worker_registration(&mut self) {
        let _ = unregister_worker_message_bus(
            self.worker.server_instance_id(),
            self.worker.worker_id(),
        );
        unset_current_message_bus();
        unset_current_worker_ring();
        unset_current_worker_local();
    }

    /// Tears down the worker registration and fails the recovery barrier so
    /// peers blocked in `worker_succeeded` wake up and abort.
    fn fail_bootstrap(&mut self) {
        self.teardown_worker_registration();
        self.recovery_coordinator.worker_failed();
    }
}

impl WorkerRingLoop {
    pub(super) fn drive_local_future<F, T>(&mut self, future: F, phase: &str) -> RS<T>
    where
        F: Future<Output = RS<T>>,
    {
        let mut future = std::pin::pin!(future);
        let waker = noop_waker();
        let mut cx = std::task::Context::from_waker(&waker);
        loop {
            match future.as_mut().poll(&mut cx) {
                std::task::Poll::Ready(result) => return result,
                std::task::Poll::Pending => self.pump_ring_once(phase)?,
            }
        }
    }

    /// Runs one submit/wait/dispatch round of the worker ring: wakes expired
    /// ring-native timeouts, submits queued user ring I/O and the ring
    /// itself, then waits for one CQE and dispatches it. A timed-out or
    /// interrupted wait returns early so the caller can re-poll its future.
    /// `phase` labels the io_uring error messages.
    fn pump_ring_once(&mut self, phase: &str) -> RS<()> {
        self.wake_expired_ring_timeouts()?;
        self.submit_user_ring_io_if_needed()?;
        let submitted = self.ring.submit();
        if submitted < 0 {
            return Err(mudu_error!(
                ErrorCode::Network,
                format!("io_uring submit error during {} {}", phase, submitted)
            ));
        }
        let cqe = match self.wait_for_cqe()? {
            Ok(cqe) => cqe,
            Err(wait_rc) if wait_rc == -libc::ETIME || wait_rc == -libc::EINTR => return Ok(()),
            Err(wait_rc) => {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!("io_uring wait cqe error during {} {}", phase, wait_rc)
                ));
            }
        };
        self.process_cqe(cqe)
    }
}

/// Drive `future` to completion using a standalone worker ring. This is used
/// before `WorkerRingLoop` is fully constructed, so that worker initialization
/// (which may open meta catalogs via the io_uring AsyncFs) can submit and
/// complete io_uring I/O.
pub(crate) fn drive_future_with_ring<F, T>(
    ring: &mut mudu_sys::io::iouring::IoUring,
    worker_local_ring: &Arc<WorkerLocalRing>,
    future: F,
    phase: &str,
) -> RS<T>
where
    F: Future<Output = RS<T>>,
{
    let mut inflight = HashMap::<u64, InflightOp>::new();
    let mut next_token: u64 = 1;
    let mut future = std::pin::pin!(future);
    let waker = noop_waker();
    let mut cx = std::task::Context::from_waker(&waker);
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(result) => return result,
            std::task::Poll::Pending => {
                let mut ctx = LoopUserIoCtx {
                    ring,
                    user_ring: worker_local_ring,
                    inflight: &mut inflight,
                    next_token: &mut next_token,
                };
                submit_user_io(&mut ctx)?;
                let submitted = ring.submit();
                if submitted < 0 {
                    return Err(mudu_error!(
                        ErrorCode::Network,
                        format!("io_uring submit error during {} {}", phase, submitted)
                    ));
                }
                let cqe = ring.wait().map_err(|wait_rc| {
                    mudu_error!(
                        ErrorCode::Network,
                        format!("io_uring wait cqe error during {} {}", phase, wait_rc)
                    )
                })?;
                let token = cqe.user_data();
                let result = cqe.result();
                let op = inflight.remove(&token).ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::Internal,
                        format!("unknown io_uring completion token {}", token)
                    )
                })?;
                match op {
                    InflightOp::UserIo(op) => {
                        handle_user_io_completion(worker_local_ring, op, result)?
                    }
                    _other => {
                        return Err(mudu_error!(
                            ErrorCode::Internal,
                            format!("unexpected non-user io completion during {}", phase)
                        ));
                    }
                }
            }
        }
    }
}

fn noop_waker() -> std::task::Waker {
    use std::task::{RawWaker, RawWakerVTable, Waker};
    unsafe fn noop_clone(_: *const ()) -> RawWaker {
        noop_raw_waker()
    }
    unsafe fn noop(_: *const ()) {}
    const VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);
    fn noop_raw_waker() -> RawWaker {
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    unsafe { Waker::from_raw(noop_raw_waker()) }
}
