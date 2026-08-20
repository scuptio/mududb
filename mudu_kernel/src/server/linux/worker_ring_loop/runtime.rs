use super::*;
use crate::server::loop_stats::{self, LoopCounter, LoopGuard, LoopPhase};
use tracing::trace;
impl WorkerRingLoop {
    /// Main poll/submit loop for the worker.
    ///
    /// Each iteration:
    /// 1. reacts to shutdown,
    /// 2. drains mailbox work,
    /// 3. advances connection tasks and log flushing,
    /// 4. submits any missing io_uring operations,
    /// 5. waits for and dispatches completions.
    ///
    /// Task polling runs in bounded slices (`TASK_POLL_BUDGET`): when more
    /// tasks are queued, the loop harvests already-available CQEs without
    /// blocking and continues polling instead of letting one long poll round
    /// delay every pending completion (e.g. a WAL fsync CQE) behind it.
    pub(super) fn run_service_loop(&mut self) -> RS<WorkerLoopStats> {
        loop {
            crate::server::stage_stats::dump_if_due(self.worker.worker_id());
            loop_stats::dump_if_due(self.worker.worker_id());
            loop_stats::count(LoopCounter::Iterations);
            if self.stop.load(Ordering::Relaxed) || self.shutdown_triggered.load(Ordering::Relaxed)
            {
                self.begin_shutdown()?;
            }
            {
                let _guard = LoopGuard::new(LoopPhase::Mailbox);
                if !self.shutting_down {
                    for msg in drain_messages(self.mailbox.as_ref(), &mut self.stats) {
                        self.handle_mailbox_message(msg)?;
                    }
                } else {
                    self.shutdown_connection_tasks()?;
                }
            }
            {
                let _guard = LoopGuard::new(LoopPhase::PollFlushLog);
                self.poll_flush_log()?;
                if self.shutting_down {
                    self.force_flush_log()?;
                }
            }
            self.worker_local_ring
                .worker_task_registry()
                .drain_completions();
            self.wake_expired_ring_timeouts()?;
            let more_ready = {
                let _guard = LoopGuard::new(LoopPhase::TaskPoll);
                self.poll_ready_worker_tasks()?
            };
            if more_ready {
                loop_stats::count(LoopCounter::TaskPollBudgetFull);
            }
            self.submit_mailbox_read_if_needed()?;
            self.submit_accept_if_needed()?;
            self.submit_fs_gc_round_if_due()?;
            self.submit_page_flush_round_if_due()?;
            self.submit_user_ring_io_if_needed()?;
            self.stats.submit_calls += 1;
            let submitted = {
                let _guard = LoopGuard::new(LoopPhase::Submit);
                self.ring.submit()
            };
            if submitted > 0 {
                loop_stats::count_by(LoopCounter::SubmitSqes, submitted as u64);
            }
            trace!(submitted, "worker_ring_loop ring.submit done");
            if submitted < 0 {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    format!("io_uring_submit error {}", submitted)
                ));
            }

            if self.shutting_down && self.worker_local_ring.worker_task_registry().is_empty() {
                // Make sure the WAL has been fully flushed before tearing the
                // worker loop down. The flush/fsync tasks are not tracked in
                // the worker task registry, so the registry being empty is
                // not enough to guarantee durability. In periodic sync mode
                // the fsync slot must also be idle and the dirty set drained:
                // start_periodic_fsync_task(force=true) keeps re-inserting
                // the fsync task during shutdown until the final fsync has
                // completed.
                let log_flushed = match &self.log {
                    Some(log) => {
                        log.backend().is_flush_idle()?
                            && log.backend().is_fsync_idle()?
                            && !log.backend().has_pending_periodic_fsync()?
                    }
                    None => true,
                };
                if log_flushed {
                    return self.finish_shutdown();
                }
            }

            if self.inflight.is_empty() {
                // No io_uring operation can complete. Only sleep when there is
                // also no queued task work; sleeping behind runnable tasks
                // would add up to 1ms of dead time to every poll slice.
                if !more_ready {
                    loop_stats::count(LoopCounter::Sleep1ms);
                    mudu_sys::task::sync::sleep_blocking(Duration::from_millis(1));
                }
                continue;
            }

            if more_ready {
                // Tasks are still queued: do not block in wait_for_cqe.
                // Harvest the completions that are already available so they
                // are observed between task-poll slices, then continue.
                self.drain_available_cqes()?;
                continue;
            }

            self.stats.wait_cqe_calls += 1;
            let cqe = {
                let _guard = LoopGuard::new(LoopPhase::WaitCqe);
                match self.wait_for_cqe()? {
                    Ok(cqe) => cqe,
                    Err(wait_rc) if wait_rc == -libc::ETIME => {
                        loop_stats::count(LoopCounter::WaitCqeEtime);
                        continue;
                    }
                    Err(wait_rc) if wait_rc == -libc::EINTR => continue,
                    Err(wait_rc) => {
                        return Err(mudu_error!(
                            ErrorCode::Network,
                            format!("io_uring_wait_cqe error {}", wait_rc)
                        ));
                    }
                }
            };
            trace!(
                user_data = cqe.user_data(),
                result = cqe.result(),
                "worker_ring_loop got cqe"
            );
            loop_stats::count(LoopCounter::DrainCqes);
            self.process_cqe(cqe)?;

            self.drain_available_cqes()?;

            // Poll the WAL flush slots once more in the same iteration: a
            // flush/fsync task whose completion was just dispatched gets its
            // next poll immediately instead of waiting for the top of the
            // next iteration.
            self.poll_flush_log()?;
        }
    }

    /// Dispatches every CQE that is already available without blocking.
    fn drain_available_cqes(&mut self) -> RS<()> {
        loop {
            let next_cqe = match self.ring.peek() {
                Ok(Some(cqe)) => cqe,
                Ok(None) => break,
                Err(peek_rc) => {
                    return Err(mudu_error!(
                        ErrorCode::Network,
                        format!("io_uring_peek_cqe error {}", peek_rc)
                    ));
                }
            };
            loop_stats::count(LoopCounter::DrainCqes);
            self.process_cqe(next_cqe)?;
        }
        Ok(())
    }

    fn begin_shutdown(&mut self) -> RS<()> {
        // Shutdown is staged: stop taking new work, close the listener, and
        // actively nudge connection tasks so they can drain and exit.
        if self.shutting_down {
            return Ok(());
        }
        self.shutting_down = true;
        self.shutdown_connection_tasks()?;
        if self.listener_fd >= 0 {
            let rc = unsafe { libc::close(self.listener_fd) };
            if rc != 0 {
                return Err(mudu_error!(
                    ErrorCode::Network,
                    "close io_uring listener during shutdown error",
                    std::io::Error::last_os_error()
                ));
            }
            self.listener_fd = -1;
        }
        Ok(())
    }

    /// Maximum number of ready worker tasks polled per service-loop
    /// iteration. Bounding the slice keeps CQE observation latency (WAL
    /// fsync, socket reads, RPC responses) bounded when the worker is busy.
    const TASK_POLL_BUDGET: usize = 8;

    pub(in crate::server) fn poll_ready_worker_tasks(&mut self) -> RS<bool> {
        let (completed, more_ready) = self
            .worker_local_ring
            .worker_task_registry()
            .poll_ready_budget(Self::TASK_POLL_BUDGET);
        for completed in completed {
            if completed.is_system() {
                if let Err(_err) = completed.into_result() {
                    // Detached system callbacks should not disrupt the worker
                    // event loop. They are fire-and-forget tasks whose errors
                    // are isolated from connection lifecycle management.
                }
                continue;
            }
            let opt_conn_id = completed.conn_id();
            match completed.into_result() {
                Ok(_) => {}
                Err(_) => {
                    if let Some(conn_id) = opt_conn_id {
                        self.worker.close_connection_sessions(conn_id)?;
                    }
                }
            }
        }
        Ok(more_ready)
    }

    fn shutdown_connection_tasks(&mut self) -> RS<()> {
        for fd in self.connection_task_fds.lock()?.values() {
            unsafe {
                libc::shutdown(*fd, libc::SHUT_RDWR);
            }
        }
        Ok(())
    }

    fn finish_shutdown(&mut self) -> RS<WorkerLoopStats> {
        self.ring.exit();
        Ok(self.stats.clone())
    }

    pub(super) fn wait_for_cqe(&mut self) -> RS<Result<mudu_sys::io::iouring::Cqe, i32>> {
        if let Some(timeout) = self.next_wait_timeout()? {
            trace!(
                timeout_us = timeout.as_micros() as u64,
                "worker_ring_loop wait_for_cqe_timeout"
            );
            return Ok(self.ring.wait_timeout(timeout));
        }
        trace!("worker_ring_loop wait_for_cqe_blocking");
        Ok(self.ring.wait())
    }

    /// The CQE wait is bounded by the nearest of the WAL flush deadline and
    /// the nearest ring-native task timeout (`WorkerLocalRing` timeout heap),
    /// so expired timeouts are noticed promptly even when no I/O completes.
    fn next_wait_timeout(&self) -> RS<Option<Duration>> {
        let mut wait = self.log_flush_wait_timeout()?;
        if let Some(deadline) = self.worker_local_ring.next_timeout_deadline()? {
            let until = deadline.saturating_duration_since(mudu_sys::time::instant_now());
            wait = Some(match wait {
                Some(log_wait) => log_wait.min(until),
                None => until,
            });
        }
        Ok(wait)
    }

    /// Wakes every task whose ring-native timeout (`async_::timeout`/`sleep`
    /// on a worker thread) has expired. Tokio timers never advance on worker
    /// threads, so this heap is the only clock that can unpark them.
    pub(super) fn wake_expired_ring_timeouts(&mut self) -> RS<()> {
        let now = mudu_sys::time::instant_now();
        for waker in self.worker_local_ring.take_expired_timeouts(now)? {
            waker.wake();
        }
        Ok(())
    }

    fn log_flush_wait_timeout(&self) -> RS<Option<Duration>> {
        let Some(log) = &self.log else {
            return Ok(None);
        };
        let mut deadline = log.backend().next_flush_deadline()?;
        // In periodic sync mode the loop must also wake when the next WAL
        // fsync comes due, even with no flush work queued.
        if let Some(fsync_deadline) = log.backend().next_periodic_fsync_deadline()? {
            deadline = Some(match deadline {
                Some(flush_deadline) => flush_deadline.min(fsync_deadline),
                None => fsync_deadline,
            });
        }
        let Some(deadline) = deadline else {
            return Ok(None);
        };
        Ok(Some(
            deadline.saturating_duration_since(*mudu_sys::time::instant_now()),
        ))
    }

    fn poll_flush_log(&mut self) -> RS<()> {
        let Some(log) = &self.log else {
            return Ok(());
        };
        // Periodic WAL fsync runs in its own task slot, decoupled from the
        // flush slot, so an in-flight fsync never blocks write rounds. The
        // order here matters:
        // 1. start_periodic_fsync_task inserts the fsync task when due (or,
        //    during shutdown, whenever dirty paths remain),
        // 2. poll_fsync_task gives the inserted task its first poll in this
        //    same loop iteration — its fsync SQEs are only submitted on
        //    first poll, and the resulting CQE is what later wakes
        //    wait_for_cqe (inserting un-polled would let the loop block
        //    forever with no SQE in flight),
        // 3. poll_flush_log drives the write path.
        log.backend()
            .start_periodic_fsync_task(self.shutting_down)?;
        log.backend().poll_fsync_task()?;
        let started = log.backend().poll_flush_log()?;
        trace!(started, "worker_ring_loop poll_flush_log result");
        Ok(())
    }

    fn force_flush_log(&mut self) -> RS<()> {
        let Some(log) = &self.log else {
            return Ok(());
        };
        let started = log.backend().force_flush_log()?;
        trace!(started, "worker_ring_loop force_flush_log result");
        Ok(())
    }
}
