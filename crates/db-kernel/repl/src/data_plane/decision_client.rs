//! Weak worker-to-group client: the group's worker ownership must not cycle.
use super::DataPlaneService;
use catalog::contract::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionClient, TxDecisionStore, TxDecisionStruct,
    TxDecisionSubmitter,
};
use catalog::contract::tx_decision_state::CoordinatorState;
use mudu::common::{id::OID, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use std::sync::{Arc, Weak};

struct Client(Weak<DataPlaneService>, u32);
struct Activity {
    _token: Arc<()>,
}
impl catalog::contract::tx_decision::CoordinatorActivity for Activity {}
impl Client {
    fn service(&self) -> RS<Arc<DataPlaneService>> {
        self.0
            .upgrade()
            .ok_or_else(|| mudu_error!(ErrorCode::ChannelClosed, "coordinator data group stopped"))
    }
}
impl DataPlaneService {
    /// Worker-facing client which does not extend the replication group's lifetime.
    pub fn decision_client(self: &Arc<Self>, worker_index: u32) -> Arc<dyn TxDecisionClient> {
        Arc::new(Client(Arc::downgrade(self), worker_index))
    }
}
#[async_trait::async_trait]
impl TxDecisionStore for Client {
    async fn decision(&self, coordinator: &CoordinatorRef) -> RS<Option<TxDecision>> {
        self.service()?.decision(coordinator).await
    }
}
#[async_trait::async_trait]
impl TxDecisionSubmitter for Client {
    async fn submit(&self, request_id: u128, record: TxDecisionStruct) -> RS<()> {
        self.service()?.submit_tx_record(request_id, record).await
    }
}
#[async_trait::async_trait]
impl TxDecisionClient for Client {
    fn coordinator_ref(&self, tx_id: OID) -> RS<CoordinatorRef> {
        Ok(self.service()?.coordinator_ref(tx_id))
    }
    fn participant_ref(
        &self,
        partition_id: OID,
    ) -> RS<catalog::contract::tx_decision::TxParticipant> {
        let owner = self.service()?.coordinator_ref(0);
        Ok(catalog::contract::tx_decision::TxParticipant {
            group_id: owner.group_id,
            group_generation: owner.group_generation,
            worker_index: self.1,
            partition_id,
        })
    }
    fn track_coordinator(
        &self,
        coordinator: &CoordinatorRef,
    ) -> RS<Box<dyn catalog::contract::tx_decision::CoordinatorActivity>> {
        let service = self.service()?;
        if service.coordinator_ref(coordinator.tx_id) != *coordinator {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "foreign coordinator activity"
            ));
        }
        let mut active = service.coordinators.lock()?;
        active.retain(|_, token| token.strong_count() > 0);
        if active.contains_key(coordinator) {
            return Err(mudu_error!(
                ErrorCode::ResourceBusy,
                "coordinator is already active"
            ));
        }
        let token = Arc::new(());
        active.insert(coordinator.clone(), Arc::downgrade(&token));
        Ok(Box::new(Activity { _token: token }))
    }
    fn coordinator_active(&self, coordinator: &CoordinatorRef) -> RS<bool> {
        let service = self.service()?;
        let active = service.coordinators.lock()?;
        Ok(active
            .get(coordinator)
            .is_some_and(|token| token.strong_count() > 0))
    }
    async fn leader_term(&self) -> RS<u64> {
        let service = self.service()?;
        mudu_sys::timeout(
            std::time::Duration::from_secs(30),
            service.registry.leader_view(),
        )
        .await
        .ok_or_else(|| mudu_error!(ErrorCode::TimedOut, "coordinator leader barrier timed out"))?
        .map(|(term, _)| term)
    }
    async fn pending_transactions(&self) -> RS<(u64, Vec<CoordinatorState>)> {
        self.service()?.pending_transactions().await
    }
    async fn transaction_state(
        &self,
        coordinator: &CoordinatorRef,
    ) -> RS<Option<CoordinatorState>> {
        self.service()?.transaction_state(coordinator).await
    }
}
