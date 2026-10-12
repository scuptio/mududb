use super::*;
use crate::utils::{single_delete_batch, single_put_batch};

impl WorkerXContract {
    /// Put a KV pair as an autocommit (a single-entry transaction): append
    /// to the WAL when a log exists, then apply.
    pub async fn worker_put_async(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "contract_worker_put_start");
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            trace.watch("put.xid", &xid.to_string());
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    Some(value.clone()),
                    single_put_batch(xid, key, value),
                ),
            )
        };
        if let Some(log) = log {
            trace.watch("put.stage", "contract_worker_put_wal_append_start");
            let route = self.repl_commit_route()?;
            // Stream-mode primary: bracket the append/apply span so the
            // snapshot watermark only claims fully applied LSNs.
            let track_ctl = self.repl_ctl()?;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_begin();
            }
            let applied: RS<()> = async {
                match &route {
                    ReplCommitRoute::Reject => return Err(not_serving_leader_error()),
                    ReplCommitRoute::Sequencer(sequencer) => {
                        self.stage_autocommit(&log, sequencer, prepared).await?;
                        trace.watch("put.stage", "contract_worker_put_wal_append_done");
                        return Ok(());
                    }
                    ReplCommitRoute::Direct | ReplCommitRoute::DirectSync(..) => {
                        new_xl_batch_writer(log.clone())
                            .append(prepared.batch())
                            .await?;
                    }
                }
                trace.watch("put.stage", "contract_worker_put_wal_append_done");
                trace.watch("put.stage", "contract_worker_put_storage_apply_start");
                storage.apply_prepared_commit_async(prepared).await?;
                Ok(())
            }
            .await;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_end(log.last_allocated_lsn().as_u64());
            }
            applied?;
            if matches!(route, ReplCommitRoute::Sequencer(_)) {
                return Ok(());
            }
            // sync replication: the commit is durable locally and applied;
            // only the replica-ack wait decides what the client hears.
            if let ReplCommitRoute::DirectSync(waiter, timeout) = &route {
                log.flush_async().await?;
                waiter
                    .wait_cover(log.last_allocated_lsn().as_u64(), *timeout)
                    .await?;
            }
            return Ok(());
        }
        trace.watch("put.stage", "contract_worker_put_storage_apply_start");
        storage.apply_prepared_commit_async(prepared).await
    }

    /// Delete a KV key as an autocommit (a single-entry transaction): append
    /// to the WAL when a log exists, then apply.
    pub async fn worker_delete_async(&self, key: &[u8]) -> RS<()> {
        let key = key.to_vec();
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    None,
                    single_delete_batch(xid, key),
                ),
            )
        };
        if let Some(log) = log {
            let route = self.repl_commit_route()?;
            let track_ctl = self.repl_ctl()?;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_begin();
            }
            let applied: RS<()> = async {
                match &route {
                    ReplCommitRoute::Reject => return Err(not_serving_leader_error()),
                    ReplCommitRoute::Sequencer(sequencer) => {
                        return self.stage_autocommit(&log, sequencer, prepared).await;
                    }
                    ReplCommitRoute::Direct | ReplCommitRoute::DirectSync(..) => {
                        new_xl_batch_writer(log.clone())
                            .append(prepared.batch())
                            .await?;
                    }
                }
                storage.apply_prepared_commit_async(prepared).await
            }
            .await;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_end(log.last_allocated_lsn().as_u64());
            }
            applied?;
            if matches!(route, ReplCommitRoute::Sequencer(_)) {
                return Ok(());
            }
            if let ReplCommitRoute::DirectSync(waiter, timeout) = &route {
                log.flush_async().await?;
                waiter
                    .wait_cover(log.last_allocated_lsn().as_u64(), *timeout)
                    .await?;
            }
            return Ok(());
        }
        storage.apply_prepared_commit_async(prepared).await
    }

    /// Read the latest committed value of a KV key.
    pub async fn worker_get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, None).await
    }

    /// Read the value of a KV key as visible to `snapshot`.
    pub async fn worker_get_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, Some(snapshot)).await
    }

    /// Scan the latest committed KV items with keys in `[start_key,
    /// end_key)` (an empty `end_key` is unbounded).
    pub async fn worker_range_scan_async(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage.kv_range(start_key, end_key, None).await
    }

    /// Scan the KV items visible to `snapshot` with keys in `[start_key,
    /// end_key)` (an empty `end_key` is unbounded).
    pub async fn worker_range_scan_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage
            .kv_range(start_key, end_key, Some(snapshot))
            .await
    }
}
