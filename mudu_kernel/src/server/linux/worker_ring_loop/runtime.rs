use super::*;
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
    pub(super) fn run_service_loop(&mut self) -> RS<WorkerLoopStats> {
        loop {
            if self.stop.load(Ordering::Relaxed) || self.shutdown_triggered.load(Ordering::Relaxed)
            {
                self.begin_shutdown()?;
            }
            if !self.shutting_down {
                for msg in drain_messages(self.mailbox.as_ref(), &mut self.stats) {
                    self.handle_mailbox_message(msg)?;
                }
            } else {
                self.shutdown_connection_tasks();
            }
            self.poll_flush_log()?;
            if self.shutting_down {
                self.force_flush_log()?;
            }
            self.worker_local_ring
                .worker_task_registry()
                .drain_completions();
            self.poll_ready_worker_tasks()?;
            self.submit_mailbox_read_if_needed()?;
            self.submit_accept_if_needed()?;
            self.submit_user_ring_io_if_needed()?;
            self.stats.submit_calls += 1;
            let submitted = self.ring.submit();
            trace!(submitted, "worker_ring_loop ring.submit done");
            if submitted < 0 {
                return Err(m_error!(
                    EC::NetErr,
                    format!("io_uring_submit error {}", submitted)
                ));
            }

            if self.shutting_down && self.worker_local_ring.worker_task_registry().is_empty() {
                // Make sure the WAL has been fully flushed before tearing the
                // worker loop down. The flush task is not tracked in the worker
                // task registry, so the registry being empty is not enough to
                // guarantee durability.
                let log_flushed = match &self.log {
                    Some(log) => log.backend().is_flush_idle()?,
                    None => true,
                };
                if log_flushed {
                    return self.finish_shutdown();
                }
            }

            if self.inflight.is_empty() {
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(1));
                continue;
            }

            self.stats.wait_cqe_calls += 1;
            let cqe = match self.wait_for_cqe()? {
                Ok(cqe) => cqe,
                Err(wait_rc) if wait_rc == -libc::ETIME => continue,
                Err(wait_rc) if wait_rc == -libc::EINTR => continue,
                Err(wait_rc) => {
                    return Err(m_error!(
                        EC::NetErr,
                        format!("io_uring_wait_cqe error {}", wait_rc)
                    ))
                }
            };
            trace!(
                user_data = cqe.user_data(),
                result = cqe.result(),
                "worker_ring_loop got cqe"
            );
            self.process_cqe(cqe)?;

            loop {
                let next_cqe = match self.ring.peek() {
                    Ok(Some(cqe)) => cqe,
                    Ok(None) => break,
                    Err(peek_rc) => {
                        return Err(m_error!(
                            EC::NetErr,
                            format!("io_uring_peek_cqe error {}", peek_rc)
                        ))
                    }
                };
                self.process_cqe(next_cqe)?;
            }
        }
    }

    fn begin_shutdown(&mut self) -> RS<()> {
        // Shutdown is staged: stop taking new work, close the listener, and
        // actively nudge connection tasks so they can drain and exit.
        if self.shutting_down {
            return Ok(());
        }
        self.shutting_down = true;
        self.shutdown_connection_tasks();
        if self.listener_fd >= 0 {
            let rc = unsafe { libc::close(self.listener_fd) };
            if rc != 0 {
                return Err(m_error!(
                    EC::NetErr,
                    "close io_uring listener during shutdown error",
                    std::io::Error::last_os_error()
                ));
            }
            self.listener_fd = -1;
        }
        Ok(())
    }

    pub(in crate::server) fn poll_ready_worker_tasks(&mut self) -> RS<()> {
        for completed in self.worker_local_ring.worker_task_registry().poll_ready() {
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
        Ok(())
    }

    fn shutdown_connection_tasks(&mut self) {
        for fd in self.connection_task_fds.lock().unwrap().values() {
            unsafe {
                libc::shutdown(*fd, libc::SHUT_RDWR);
            }
        }
    }

    fn finish_shutdown(&mut self) -> RS<WorkerLoopStats> {
        self.ring.exit();
        Ok(self.stats.clone())
    }

    fn wait_for_cqe(&mut self) -> RS<Result<mudu_sys::io::iouring::Cqe, i32>> {
        if let Some(timeout) = self.log_flush_wait_timeout()? {
            trace!(
                timeout_us = timeout.as_micros() as u64,
                "worker_ring_loop wait_for_cqe_timeout"
            );
            return Ok(self.ring.wait_timeout(timeout));
        }
        trace!("worker_ring_loop wait_for_cqe_blocking");
        Ok(self.ring.wait())
    }

    fn log_flush_wait_timeout(&self) -> RS<Option<Duration>> {
        let Some(log) = &self.log else {
            return Ok(None);
        };
        let Some(deadline) = log.backend().next_flush_deadline()? else {
            return Ok(None);
        };
        Ok(Some(
            deadline.saturating_duration_since(mudu_sys::time::instant_now()),
        ))
    }

    fn poll_flush_log(&mut self) -> RS<()> {
        let Some(log) = &self.log else {
            return Ok(());
        };
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
