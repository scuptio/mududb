//! Transaction state and command results reconstructed from one durable prefix.
use super::machine::TX_DECISION_STREAM;
use super::view::View;
use crate::log_stream::{DurableStreamLog, FrontierStore, StreamStateMachine, StreamStruct};
use catalog::contract::tx_decision::{CoordinatorRef, TxDecisionError, TxDecisionStruct};
use catalog::contract::tx_decision_state::{transition, CoordinatorState};
use mudu::common::{id::OID, result::RS};
use mudu::{error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamProgress};
use mudu_sys::sync::SMutex;

pub(super) struct TxDecisionMachine {
    identity: GroupIdobject,
    frontier: FrontierStore,
    view: SMutex<View>,
    owner: tokio::sync::Mutex<()>,
    changed: tokio::sync::Notify,
    checkpoint_path: std::path::PathBuf,
}

impl TxDecisionMachine {
    pub(super) async fn open(log: &DurableStreamLog, dir: &std::path::Path) -> RS<Self> {
        let (frontier, recovered) =
            FrontierStore::open(dir, "tx_decision.frontier", TX_DECISION_STREAM, 1).await?;
        let identity = log.identity();
        let checkpoint_path = dir.join("tx_decision.snapshot");
        let mut view =
            super::tx_snapshot::load(&checkpoint_path, identity, log, recovered.as_ref()).await?;
        if let Some((progress, _)) = recovered {
            if !log.contains(&progress).await? {
                return Err(invalid(
                    "decision frontier is not covered by durable history",
                ));
            }
            let start = match &view.applied {
                Some(prefix) => prefix
                    .end_lsn
                    .checked_add(1)
                    .ok_or_else(|| invalid("decision LSN exhausted"))?,
                None => 0,
            };
            for lsn in start..=progress.end_lsn {
                let (_, record) = log.record(lsn).await?;
                let command: TxDecisionStruct =
                    crate::log_stream::frontier::decode(&record.payload)?;
                let next = evaluate(identity, &view, &command);
                publish(&mut view, lsn, command.coordinator().tx_id, next);
            }
            view.applied = Some(progress);
        }
        Ok(Self {
            identity,
            frontier,
            view: SMutex::new(view),
            owner: tokio::sync::Mutex::new(()),
            changed: tokio::sync::Notify::new(),
            checkpoint_path,
        })
    }

    pub(super) fn state(&self, coordinator: &CoordinatorRef) -> RS<Option<CoordinatorState>> {
        if !belongs(self.identity, coordinator) {
            return Err(invalid("foreign coordinator group"));
        }
        Ok(self
            .view
            .lock()?
            .transactions
            .get(&coordinator.tx_id)
            .cloned())
    }

    pub(super) fn pending(&self) -> RS<Vec<CoordinatorState>> {
        Ok(self
            .view
            .lock()?
            .transactions
            .values()
            .filter(|state| !state.complete)
            .cloned()
            .collect())
    }

    pub(super) fn result(&self, lsn: u64) -> RS<Option<Result<(), TxDecisionError>>> {
        Ok(self.view.lock()?.results.get(&lsn).copied())
    }

    pub(super) fn changed(&self) -> &tokio::sync::Notify {
        &self.changed
    }

    /// Atomically checkpoints the applied state and every retained command
    /// result. This does not authorize log GC or replace a group manifest.
    pub(super) async fn checkpoint(&self) -> RS<Option<StreamProgress>> {
        let _owner = self.owner.lock().await;
        self.checkpoint_owned().await
    }

    async fn checkpoint_owned(&self) -> RS<Option<StreamProgress>> {
        let view = self.view.lock()?.clone();
        super::tx_snapshot::store(&self.checkpoint_path, self.identity, &view).await?;
        Ok(view.applied)
    }
}

#[async_trait::async_trait]
impl StreamStateMachine for TxDecisionMachine {
    async fn snapshot_at(&self, prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        let _owner = self.owner.lock().await;
        let view = self.view.lock()?.clone();
        if view.applied.as_ref() != prefix {
            return Err(invalid("decision snapshot requires exact applied prefix"));
        }
        super::tx_snapshot::encode_image(self.identity, &view)
    }

    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self.view.lock()?.applied.clone())
    }

    async fn apply_once(&self, _request_id: u128, record: StreamStruct) -> RS<()> {
        let _owner = self.owner.lock().await;
        if record.identity != self.identity
            || record.progress.stream_id != TX_DECISION_STREAM
            || record.progress.stream_generation != 1
        {
            return Err(invalid("foreign decision stream"));
        }
        let command: TxDecisionStruct = crate::log_stream::frontier::decode(&record.payload)?;
        let next = {
            let view = self.view.lock()?;
            if view.applied.as_ref() == Some(&record.progress) {
                return Ok(());
            }
            let expected = match &view.applied {
                Some(previous) => previous
                    .end_lsn
                    .checked_add(1)
                    .ok_or_else(|| invalid("decision LSN exhausted"))?,
                None => 0,
            };
            if record.progress.end_lsn != expected {
                return Err(invalid("decision apply is not contiguous"));
            }
            evaluate(self.identity, &view, &command)
        };
        // The immutable log plus this atomic prefix is the durable transaction:
        // replay reconstructs BOTH business state and rejection results. A
        // cancellation before publication leaves memory untouched and retryable.
        self.frontier.store(&record.progress, &[]).await?;
        let checkpoint_due = record.progress.end_lsn % 256 == 255;
        {
            let mut view = self.view.lock()?;
            publish(
                &mut view,
                record.progress.end_lsn,
                command.coordinator().tx_id,
                next,
            );
            view.applied = Some(record.progress);
        }
        self.changed.notify_waiters();
        if checkpoint_due {
            // The command is already durable. Checkpoint failure must not
            // change its result; recovery can still replay the retained log.
            if let Err(error) = self.checkpoint_owned().await {
                tracing::warn!("decision checkpoint deferred: {error}");
            }
        }
        Ok(())
    }
}

pub(super) fn evaluate(
    identity: GroupIdobject,
    view: &View,
    command: &TxDecisionStruct,
) -> Result<CoordinatorState, TxDecisionError> {
    if !belongs(identity, command.coordinator()) {
        return Err(TxDecisionError::ForeignCoordinator);
    }
    transition(view.transactions.get(&command.coordinator().tx_id), command)
}

fn belongs(identity: GroupIdobject, coordinator: &CoordinatorRef) -> bool {
    identity.cluster_id == coordinator.cluster_id
        && identity.group_id.0 == coordinator.group_id
        && identity.generation == coordinator.group_generation
}

pub(super) fn publish(
    view: &mut View,
    lsn: u64,
    tx_id: OID,
    next: Result<CoordinatorState, TxDecisionError>,
) {
    let result = next.map(|state| {
        view.transactions.insert(tx_id, state);
    });
    view.results.insert(lsn, result);
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}
