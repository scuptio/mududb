use super::*;
use crate::contract::WorkerXContract;
use crate::partition_rpc::PartitionRpcRequest;

impl WorkerXContract {
    /// Register this worker's partition-RPC service on the worker channel
    /// exactly once (later calls are no-ops).
    pub fn ensure_partition_rpc_handler(self: &Arc<Self>) -> RS<()> {
        if self.partition_rpc_registered.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(
            worker_id = self.worker_id,
            "registering partition rpc handler"
        );
        let channel = current_channel()?;
        let contract = self.clone();
        let router =
            ServiceRouter::new().handle::<PartitionRpcRequest, _, _>(move |request, ctx| {
                let contract = contract.clone();
                async move { contract.handle_partition_rpc_message(request, ctx).await }
            })?;
        let handle = channel.serve(router)?;
        let mut guard = self.rpc_service.lock()?;
        *guard = Some(handle);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]
    use super::*;
    use catalog_testing::test_meta_mgr::TestMetaMgr;
    use mudu::channel::{EndpointId, Envelope, MessageId, MessageKind, RecvFilter};
    use mudu_sys::env_var::temp_dir;
    use mudu_utils::channel::{
        set_current_channel, unset_current_channel, ChannelEndpoint, ChannelEndpointRef,
        KindHandler,
    };
    use mudu_utils::oid::gen_oid;

    struct StructingChannel {
        registered: SMutex<usize>,
    }

    #[async_trait::async_trait]
    impl ChannelEndpoint for StructingChannel {
        fn local_endpoint(&self) -> EndpointId {
            0
        }
        async fn send(&self, _dst: EndpointId, _message: OutgoingMessage) -> RS<MessageId> {
            unimplemented!()
        }
        async fn recv(&self, _filter: RecvFilter, _deadline_ms: Option<u64>) -> RS<Envelope> {
            unimplemented!()
        }
        fn register_kind_handler(&self, _kind: MessageKind, _handler: KindHandler) -> RS<()> {
            *self.registered.lock().unwrap() += 1;
            Ok(())
        }
        fn unregister_kind_handler(&self, _kind: MessageKind) -> RS<()> {
            Ok(())
        }
    }

    #[test]
    fn ensure_partition_rpc_handler_registers_exactly_once() {
        let data_dir = temp_dir()
            .join(format!("rpc_register_test_{}", gen_oid()))
            .to_string_lossy()
            .to_string();
        let contract = Arc::new(
            WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
                meta_mgr: Arc::new(TestMetaMgr::new()),
                log: None,
                log_layout: Default::default(),
                active_sessions: Default::default(),
                worker_id: 7,
                default_unpartitioned_worker_id: 7,
                partition_id: 0,
                data_dir,
                async_runtime: None,
                server_instance_id: 0,
                cluster_router: None,
                epoch_oracle: None,
            })
            .unwrap(),
        );
        let recording = Arc::new(StructingChannel {
            registered: SMutex::new(0),
        });
        let channel: ChannelEndpointRef = recording.clone();
        set_current_channel(channel);
        contract.ensure_partition_rpc_handler().unwrap();
        // Idempotent: the second call is a no-op.
        contract.ensure_partition_rpc_handler().unwrap();
        let recorded = *recording.registered.lock().unwrap();
        assert_eq!(recorded, 1);
        unset_current_channel();
    }
}
