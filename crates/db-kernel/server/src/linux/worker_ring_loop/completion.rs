use super::*;

impl WorkerRingLoop {
    pub(crate) fn process_cqe(&mut self, cqe: mudu_sys::io::iouring::Cqe) -> RS<()> {
        let token = cqe.user_data();
        let result = cqe.result();
        let op = self.inflight.remove(&token).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                format!("unknown io_uring completion token {}", token)
            )
        })?;

        // Completion dispatch is token-based: each submitted SQE inserts a
        // matching inflight entry, and the CQE result is routed here.
        match op {
            InflightOp::Accept(op) => {
                self.stats.cqe_accept += 1;
                self.accept_submitted = false;
                if result >= 0 {
                    let conn_fd = result as RawFd;
                    let remote_addr = server_iouring::sockaddr_to_socket_addr(op.addr())?;
                    mudu_sys::io::net::set_tcp_nodelay(conn_fd)?;
                    let conn_id = self.conn_id_alloc.fetch_add(1, Ordering::Relaxed);
                    self.register_connection(conn_id, conn_fd, remote_addr)?;
                }
            }
            InflightOp::MailboxRead { .. } => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    mailbox_fd = self.mailbox_fd,
                    result,
                    "worker_ring_loop mailbox read cqe"
                );
                handle_read_completion(
                    self.worker.worker_id(),
                    self.mailbox_fd,
                    &mut self.mailbox_read_submitted,
                    &mut self.stats,
                );
                for msg in drain_messages(self.mailbox.as_ref(), &mut self.stats) {
                    self.handle_mailbox_message(msg)?;
                }
            }
            InflightOp::UserIo(op) => {
                let op_id = op.op_id();
                let op_kind = op.kind();
                if let Some(task_id) = self.worker_local_ring.task_for_op(op_id) {
                    if let Some(ctx) = TaskContext::get(task_id) {
                        ctx.watch("io.last_op_id", &op_id.to_string());
                        ctx.watch("io.last_op_kind", op_kind);
                        ctx.watch("io.last_cqe_token", &token.to_string());
                        ctx.watch("io.last_result", &result.to_string());
                    }
                }
                handle_user_io_completion(&self.worker_local_ring, op, result)?
            }
        }
        Ok(())
    }

    pub(super) fn handle_mailbox_message(&self, msg: WorkerMailboxMsg) -> RS<()> {
        match msg {
            WorkerMailboxMsg::BusMessage(envelope) => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    src = ?envelope.src(),
                    dst = ?envelope.dst(),
                    kind = ?envelope.kind(),
                    msg_id = envelope.msg_id(),
                    correlation_id = ?envelope.correlation_id(),
                    "worker_ring_loop received bus mailbox message"
                );
                self.message_bus.handle_incoming(envelope)?;
                debug!(
                    worker_id = self.worker.worker_id(),
                    "worker_ring_loop handled bus mailbox message"
                );
            }
            WorkerMailboxMsg::Shutdown => {
                debug!(
                    worker_id = self.worker.worker_id(),
                    "worker_ring_loop received shutdown mailbox message"
                );
                self.shutdown_triggered.store(true, Ordering::Relaxed);
            }
        }
        Ok(())
    }

    pub(crate) fn register_connection(
        &mut self,
        conn_id: u64,
        fd: RawFd,
        remote_addr: std::net::SocketAddr,
    ) -> RS<()> {
        self.stats.local_register += 1;
        self.start_connection_task(conn_id, fd, remote_addr, None)
    }

    fn start_connection_task(
        &self,
        conn_id: u64,
        fd: RawFd,
        remote_addr: std::net::SocketAddr,
        initial_response: Option<Vec<u8>>,
    ) -> RS<()> {
        let socket = mudu_sys::io::socket::IoSocket::from_raw_fd(fd);
        let _ = self.connection_task_fds.lock()?.insert(conn_id, fd);
        task::spawn(
            Some(conn_id),
            spawn_connection_worker_task(
                self.worker.clone(),
                self.connection_task_fds.clone(),
                conn_id,
                socket,
                remote_addr,
                initial_response,
            ),
        );
        Ok(())
    }
}
