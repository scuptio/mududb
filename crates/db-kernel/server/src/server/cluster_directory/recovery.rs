//! Validate the directory projection against authoritative control history.
use super::*;
use mudu_raft::stream::StreamProgress;
use repl::log_stream::DurableStreamLog;

impl ClusterDirectory {
    /// Startup-only proof. The caller owns the offline directory and supplies
    /// the committed vector recovered from Raft, never a speculative log tail.
    pub(crate) async fn validate_control_recovery(
        &self,
        log: &DurableStreamLog,
        applied: Option<&StreamProgress>,
        committed: Option<&StreamProgress>,
    ) -> RS<()> {
        let cursor = self.overlay_owner.lock().await;
        if self.overlay_failed.swap(true, Ordering::AcqRel)
            || log.identity().cluster_id != self.topology.control_cluster_id()
        {
            return Err(invalid(
                "directory recovery identity or publication state mismatch",
            ));
        }
        validate_applied(log, applied, committed).await?;
        let end = match (applied, cursor.as_ref()) {
            (Some(prefix), Some(cursor)) if cursor.version > prefix.end_lsn => {
                validate_pending(log, applied, committed, cursor.version).await?;
                Some(cursor.version)
            }
            (None, Some(cursor)) => {
                validate_pending(log, applied, committed, cursor.version).await?;
                Some(cursor.version)
            }
            (Some(prefix), _) => Some(prefix.end_lsn),
            (None, None) => None,
        };
        // Replay only the placement projection into an isolated in-memory
        // directory. No catalog mutation or durable file write is performed.
        let expected = ClusterDirectory::new(self.topology.clone(), self.self_node_id)?;
        if let Some(end) = end {
            for lsn in 0..=end {
                let (_, record) = log.record(lsn).await?;
                let op: MetaOp = rmp_serde::from_slice(&record.payload)
                    .map_err(|e| mudu_error!(ErrorCode::Decode, "decode placement history", e))?;
                expected.apply_placement_op(&op, lsn).await?;
            }
        }
        if *cursor != *expected.overlay_owner.lock().await
            || *self.overrides.lock()? != *expected.overrides.lock()?
        {
            return Err(invalid(
                "directory image does not match applied control history",
            ));
        }
        self.overlay_failed.store(false, Ordering::Release);
        Ok(())
    }
}

async fn validate_applied(
    log: &DurableStreamLog,
    applied: Option<&StreamProgress>,
    committed: Option<&StreamProgress>,
) -> RS<()> {
    let descriptor = log.descriptor();
    if descriptor.stream_id.0 != catalog::contract::control_plane::CONTROL_STREAM_META
        || descriptor.stream_generation != 1
    {
        return Err(invalid("directory recovery requires the metadata stream"));
    }
    if let Some(prefix) = applied {
        if !committed.is_some_and(|cut| {
            cut.stream_id == prefix.stream_id
                && cut.stream_generation == prefix.stream_generation
                && (cut.end_lsn > prefix.end_lsn || cut == prefix)
        }) || !log.contains(prefix).await?
        {
            return Err(invalid(
                "directory applied prefix is outside committed durable history",
            ));
        }
    }
    Ok(())
}

async fn validate_pending(
    log: &DurableStreamLog,
    applied: Option<&StreamProgress>,
    committed: Option<&StreamProgress>,
    version: u64,
) -> RS<()> {
    let next = match applied {
        Some(prefix) => prefix
            .end_lsn
            .checked_add(1)
            .ok_or_else(|| invalid("metadata LSN exhausted"))?,
        None => 0,
    };
    let cut =
        committed.ok_or_else(|| invalid("directory has effects without committed history"))?;
    if version != next || version > cut.end_lsn || !log.contains(cut).await? {
        return Err(invalid(
            "directory effect is not the next committed metadata operation",
        ));
    }
    Ok(())
}
fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}
