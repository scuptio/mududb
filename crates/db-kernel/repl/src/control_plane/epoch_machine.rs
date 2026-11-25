//! Epoch checkpoints are determined by committed history, not local allocators.
use super::{epoch_snapshot::EpochImage, frontier_checkpoint::EpochCheckpoint};
use crate::log_stream::{
    frontier::{decode, encode},
    DurableStreamLog, FrontierStore, StreamStateMachine, StreamStruct,
};
use catalog::contract::{control_plane::CONTROL_STREAM_EPOCH, epoch_oracle::ClusterEpochState};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamId, StreamProgress};
use mudu_sys::sync::SMutex;
use std::{
    path::Path,
    sync::{atomic::Ordering, Arc},
};

pub(super) struct EpochStreamMachine {
    identity: GroupIdobject,
    frontier: FrontierStore,
    image: SMutex<EpochImage>,
    owner: tokio::sync::Mutex<()>,
    pub(super) state: Arc<ClusterEpochState>,
}

impl EpochStreamMachine {
    pub(super) async fn open(
        log: &DurableStreamLog,
        dir: &Path,
        state: Arc<ClusterEpochState>,
    ) -> RS<Self> {
        if log.descriptor() != &super::streams::epoch_stream_descriptor() {
            return Err(invalid("epoch recovery stream mismatch"));
        }
        let (frontier, recovered) =
            FrontierStore::open(dir, "epoch.frontier", StreamId(CONTROL_STREAM_EPOCH), 1).await?;
        let mut image = EpochImage::empty();
        if let Some((prefix, business)) = recovered {
            if !log.contains(&prefix).await? {
                return Err(invalid("epoch frontier is not covered by durable history"));
            }
            let mut replay = EpochCheckpoint::initial();
            for lsn in 0..=prefix.end_lsn {
                let (_, record) = log.record(lsn).await?;
                replay = replay.advance(decode(&record.payload)?)?;
            }
            let checkpoint: EpochCheckpoint = decode(&business)?;
            image = EpochImage {
                version: 1,
                prefix: Some(prefix),
                checkpoint,
            };
            // The sparse in-memory advance may move the checkpoint ahead of the
            // committed history; recovery accepts a checkpoint that covers the
            // replay rather than requiring byte equality.
            if !image.checkpoint.covers(&replay) {
                return Err(invalid("epoch checkpoint regresses committed history"));
            }
        }
        let machine = Self {
            identity: log.identity(),
            frontier,
            image: SMutex::new(image.clone()),
            owner: tokio::sync::Mutex::new(()),
            state,
        };
        machine.publish(&image.checkpoint);
        Ok(machine)
    }

    fn publish(&self, checkpoint: &EpochCheckpoint) {
        self.state
            .current
            .fetch_max(checkpoint.current, Ordering::AcqRel);
        self.state
            .stable
            .fetch_max(checkpoint.stable, Ordering::AcqRel);
        self.state.stable_notify.notify_waiters();
    }

    /// Persists the current in-memory checkpoint at the last applied frontier,
    /// so a restart resumes the sparse advance instead of regressing to the
    /// last committed record (E-07). No new stream record is appended.
    pub(super) async fn checkpoint(&self) -> RS<()> {
        let _owner = self.owner.lock().await;
        let image = self.image.lock()?.clone();
        if let Some(prefix) = &image.prefix {
            self.frontier
                .store(prefix, &encode(&image.checkpoint)?)
                .await?;
        }
        Ok(())
    }

    /// The current in-memory checkpoint (stable/current/reserved_upper), for
    /// the ticker's reserve-replenishment and exhaustion checks.
    pub(super) fn checkpoint_state(&self) -> RS<EpochCheckpoint> {
        Ok(self.image.lock()?.checkpoint.clone())
    }

    /// Advances the in-memory `stable`/`current` within the reserved range and
    /// publishes them to the shared oracle state, without a committed record
    /// (E-06). Fails closed at the reserved upper: the leader must commit a
    /// fresh reserve before handing out more epochs.
    pub(super) async fn advance_in_memory(
        &self,
        stable: u64,
        current: u64,
        physical_millis: u64,
    ) -> RS<()> {
        let _owner = self.owner.lock().await;
        let image = self.image.lock()?.clone();
        let checkpoint = image
            .checkpoint
            .advance_in_memory(stable, current, physical_millis)?;
        *self.image.lock()? = EpochImage {
            version: 1,
            prefix: image.prefix,
            checkpoint: checkpoint.clone(),
        };
        self.publish(&checkpoint);
        Ok(())
    }
}

#[async_trait::async_trait]
impl StreamStateMachine for EpochStreamMachine {
    async fn applied_frontier(&self) -> RS<Option<StreamProgress>> {
        Ok(self.image.lock()?.prefix.clone())
    }

    async fn snapshot_at(&self, prefix: Option<&StreamProgress>) -> RS<Vec<u8>> {
        let _owner = self.owner.lock().await;
        let image = self.image.lock()?.clone();
        if image.prefix.as_ref() != prefix {
            return Err(invalid("epoch snapshot cut differs from applied prefix"));
        }
        encode(&image)
    }

    async fn apply_once(&self, _request_id: u128, record: StreamStruct) -> RS<()> {
        let _owner = self.owner.lock().await;
        let previous = self.image.lock()?.clone();
        if record.identity != self.identity
            || record.progress.stream_id != StreamId(CONTROL_STREAM_EPOCH)
            || record.progress.stream_generation != 1
        {
            return Err(invalid("epoch apply identity mismatch"));
        }
        if previous.prefix.as_ref() == Some(&record.progress) {
            return Ok(());
        }
        let next = match &previous.prefix {
            Some(prefix) => prefix
                .end_lsn
                .checked_add(1)
                .ok_or_else(|| invalid("epoch LSN exhausted"))?,
            None => 0,
        };
        if record.progress.end_lsn != next {
            return Err(invalid("epoch apply is not contiguous"));
        }
        let checkpoint = previous.checkpoint.advance(decode(&record.payload)?)?;
        self.frontier
            .store(&record.progress, &encode(&checkpoint)?)
            .await?;
        *self.image.lock()? = EpochImage {
            version: 1,
            prefix: Some(record.progress),
            checkpoint: checkpoint.clone(),
        };
        self.publish(&checkpoint);
        Ok(())
    }
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}
