use super::*;

impl WorkerRingLoop {
    pub(crate) fn submit_accept_if_needed(&mut self) -> RS<()> {
        if self.shutting_down || self.accept_submitted || self.listener_fd < 0 {
            return Ok(());
        }
        let token = self.alloc_token();
        let Some(mut sqe) = self.ring.next_sqe() else {
            return Ok(());
        };
        let mut op = Box::new(AcceptOp::new(
            mudu_sys::io::iouring::SockAddrBuf::new_empty(),
        ));
        sqe.set_user_data(token);
        sqe.prep_accept(self.listener_fd, op.addr_mut(), 0);
        self.inflight.insert(token, InflightOp::Accept(op));
        self.accept_submitted = true;
        self.stats.accept_submit += 1;
        Ok(())
    }

    /// Spawn one fs GC round when the interval has elapsed.
    ///
    /// io_uring worker tasks cannot sleep on the tokio timer (the service
    /// loop drives the ring synchronously), so the periodic trigger lives in
    /// the service loop: once `FS_GC_INTERVAL` has passed since the previous
    /// round, a one-round task is spawned and the next round is re-armed.
    /// Shutdown needs no extra signal: no new round is spawned once
    /// `shutting_down`/`stop` is set, and an in-flight round completes on
    /// its own, so the worker task registry still drains.
    pub(crate) fn submit_fs_gc_round_if_due(&mut self) -> RS<()> {
        if self.shutting_down || self.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = mudu_sys::time::instant_now();
        if *now < *self.fs_gc_next_due || self.fs_gc_inflight.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.fs_gc_next_due = now + FS_GC_INTERVAL;
        self.fs_gc_inflight.store(true, Ordering::Relaxed);
        let gc = self.worker.fs_gc();
        let inflight = self.fs_gc_inflight.clone();
        let worker_id = self.worker.worker_id();
        self.spawn(None, async move {
            if let Err(err) = gc.gc_round().await {
                error!(worker_id, "fs gc round failed, {}", err);
            }
            inflight.store(false, Ordering::Relaxed);
            Ok(())
        });
        Ok(())
    }

    /// Spawn one deferred data-page flush round when the interval has
    /// elapsed; same cadence mechanism as `submit_fs_gc_round_if_due`
    /// (io_uring worker tasks cannot sleep on the tokio timer). Unlike fs
    /// GC this also runs while shutting down: the flush task is tracked by
    /// the worker task registry, so the shutdown drain waits for it and the
    /// data files are clean before the loop exits.
    pub(crate) fn submit_page_flush_round_if_due(&mut self) -> RS<()> {
        if self.stop.load(Ordering::Relaxed) {
            return Ok(());
        }
        let now = mudu_sys::time::instant_now();
        if *now < *self.page_flush_next_due || self.page_flush_inflight.load(Ordering::Relaxed) {
            return Ok(());
        }
        self.page_flush_next_due = now + DIRTY_PAGE_FLUSH_INTERVAL;
        self.page_flush_inflight.store(true, Ordering::Relaxed);
        let worker = self.worker.clone();
        let inflight = self.page_flush_inflight.clone();
        let worker_id = self.worker.worker_id();
        self.spawn(None, async move {
            if let Err(err) = flush_worker_dirty_pages(&worker).await {
                error!(worker_id, "page flush round failed, {}", err);
            }
            inflight.store(false, Ordering::Relaxed);
            Ok(())
        });
        Ok(())
    }

    pub(crate) fn submit_mailbox_read_if_needed(&mut self) -> RS<()> {
        debug!(
            worker_id = self.worker.worker_id(),
            mailbox_fd = self.mailbox_fd,
            mailbox_read_submitted = self.mailbox_read_submitted,
            shutting_down = self.shutting_down,
            "worker_ring_loop submit_mailbox_read_if_needed"
        );
        let mut ctx = LoopMailboxSubmitCtx {
            worker_id: self.worker.worker_id(),
            ring: &mut self.ring,
            mailbox_fd: self.mailbox_fd,
            mailbox_read_submitted: &mut self.mailbox_read_submitted,
            inflight: &mut self.inflight,
            next_token: &mut self.next_token,
            stats: &mut self.stats,
            shutting_down: self.shutting_down,
        };
        submit_read_if_needed(&mut ctx)
    }

    pub(crate) fn submit_user_ring_io_if_needed(&mut self) -> RS<()> {
        let mut ctx = LoopUserIoCtx {
            ring: &mut self.ring,
            user_ring: &self.worker_local_ring,
            inflight: &mut self.inflight,
            next_token: &mut self.next_token,
        };
        submit_user_io(&mut ctx)
    }

    pub(crate) fn alloc_token(&mut self) -> u64 {
        let token = self.next_token;
        self.next_token += 1;
        token
    }
}
