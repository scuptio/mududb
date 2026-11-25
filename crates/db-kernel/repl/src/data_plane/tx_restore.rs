//! Offline decision recovery, with business state proved by full log replay.
use super::{service::tx_decision_descriptor, tx_machine, tx_snapshot, view, TX_DECISION_STREAM};
use crate::log_stream::{frontier::encode, DurableStreamLog, FrontierStore, LoggedStreamSnapshot};
use catalog::contract::tx_decision::TxDecisionStruct;
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamProgress};
use std::path::Path;

/// A decision image whose transactions and command results match its log.
/// Validation performs no filesystem writes. Group-level restore must validate
/// every other stream and fence service admission before installing any image.
pub struct DecisionSnapshot {
    identity: GroupIdobject,
    image: LoggedStreamSnapshot,
    view: view::View,
}

impl DecisionSnapshot {
    /// Validates identity, exact manifest cut, log integrity and business replay.
    pub fn decode(
        bytes: &[u8],
        identity: GroupIdobject,
        prefix: Option<&StreamProgress>,
    ) -> RS<Self> {
        let image =
            LoggedStreamSnapshot::decode(bytes, identity, &tx_decision_descriptor(), prefix)?;
        let view = tx_snapshot::decode_image(identity, image.business())?;
        let mut replay = view::View::default();
        image.log().visit_records(|body, progress| {
            let command: TxDecisionStruct = crate::log_stream::frontier::decode(&body.payload)?;
            let next = tx_machine::evaluate(identity, &replay, &command);
            tx_machine::publish(
                &mut replay,
                progress.end_lsn,
                command.coordinator().tx_id,
                next,
            );
            replay.applied = Some(progress);
            Ok(())
        })?;
        if encode(&view)? != encode(&replay)? {
            return Err(invalid(
                "decision image differs from its committed log replay",
            ));
        }
        Ok(Self {
            identity,
            image,
            view,
        })
    }

    /// Installs into exclusively owned, offline directories. Existing state must
    /// be absent or an identical interrupted install. Never use on a live group.
    /// Log durability precedes the frontier; startup can replay the entire log
    /// if interrupted before checkpoint publication. This is not an atomic group
    /// install: keep admission fenced until all streams and Raft are restored.
    /// After an I/O failure, drain pending log writes before reopening or retrying.
    pub async fn restore(&self, log: &DurableStreamLog, apply_dir: &Path) -> RS<()> {
        if log.identity() != self.identity || log.descriptor() != &tx_decision_descriptor() {
            return Err(invalid("decision restore target group or stream mismatch"));
        }
        let (frontier, current) =
            FrontierStore::open(apply_dir, "tx_decision.frontier", TX_DECISION_STREAM, 1).await?;
        if let Some((prefix, business)) = &current {
            if Some(prefix) != self.view.applied.as_ref() || !business.is_empty() {
                return Err(invalid(
                    "decision restore would overwrite an existing frontier",
                ));
            }
        }
        let fs = mudu_sys::fs::tokio_fs();
        let checkpoint = apply_dir.join("tx_decision.snapshot");
        if fs.path_exists(&checkpoint).await?
            && (self.view.applied.is_none()
                || fs.metadata_len(&checkpoint).await? != self.image.business().len() as u64
                || fs.read_all(&checkpoint).await? != self.image.business())
        {
            return Err(invalid(
                "decision restore would overwrite an existing checkpoint",
            ));
        }
        log.restore_snapshot(self.image.log()).await?;
        if let Some(prefix) = &self.view.applied {
            frontier.store(prefix, &[]).await?;
            tx_snapshot::store(&checkpoint, self.identity, &self.view).await?;
        }
        Ok(())
    }
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}
