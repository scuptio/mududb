use super::*;

impl WorkerXContract {
    /// Register this worker's partition-RPC handler on the message bus
    /// exactly once (later calls are no-ops).
    pub fn ensure_partition_rpc_handler(self: &Arc<Self>) -> RS<()> {
        if self.partition_rpc_registered.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(
            worker_id = self.worker_id,
            "registering partition rpc handler"
        );
        let bus = current_message_bus()?;
        let contract = self.clone();
        bus.on_recv_callback(
            RecvFilter {
                dst: Some(self.worker_id),
                kind: Some(PARTITION_RPC_REQUEST_KIND),
                ..RecvFilter::default()
            },
            Arc::new(move |envelope| {
                let contract = contract.clone();
                Box::pin(async move { contract.handle_partition_rpc(envelope).await })
            }),
        )?;
        Ok(())
    }
}
