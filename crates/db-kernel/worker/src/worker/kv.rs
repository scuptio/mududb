use super::*;
use ::catalog::contract::worker_snapshot::KvItem;
use ::conn::worker_local::WorkerExecute;
use mudu_utils::task_trace;
use std::collections::BTreeMap;

impl WorkerRuntime {
    /// Read `key` within the session owned by connection `conn_id` (fails
    /// when another connection owns the session). Sees the session
    /// transaction's staged writes when one is active.
    pub async fn get_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        self.get_in_session(session_id, key).await
    }

    /// Put a KV pair within the session owned by connection `conn_id`
    /// (fails when another connection owns the session). Stages into the
    /// session transaction when one is active, else autocommits.
    pub async fn put_for_connection_async(
        &self,
        conn_id: u64,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "worker_put_for_connection_start");
        trace.watch("put.conn_id", &conn_id.to_string());
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        trace.watch("put.stage", "worker_put_for_connection_owned");
        self.put_in_session_async(session_id, key, value).await
    }

    /// Scan the KV items with keys in `[start_key, end_key)` within the
    /// session owned by connection `conn_id` (fails when another connection
    /// owns the session). Sees the session transaction's staged writes when
    /// one is active.
    pub async fn range_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        self.range_in_session(session_id, start_key, end_key).await
    }

    pub(crate) async fn execute_tx_async(
        &self,
        session_id: OID,
        instruction: WorkerExecute,
    ) -> RS<()> {
        match instruction {
            WorkerExecute::BeginTx => {
                // Read-your-writes: wait out the session's recorded commit
                // watermark before the fresh transaction registers (epoch
                // design doc §5.2).
                self.wait_session_commit_epoch(session_id).await?;
                self.session_manager
                    .begin_session_tx(session_id, self.contract.worker_begin_tx()?)
            }
            WorkerExecute::CommitTx => {
                let tx_manager = self.session_manager.take_session_tx(session_id)?;
                // A commit error must not leak the transaction's epoch
                // registration: abort it before propagating.
                match self
                    .contract
                    .worker_commit_routed_tx_async(tx_manager.clone())
                    .await
                {
                    Ok(committed) => self.note_session_commit_epoch(session_id, committed),
                    Err(err) => {
                        let _ = self.contract.worker_abort_tx_async(tx_manager).await;
                        Err(err)
                    }
                }
            }
            WorkerExecute::RollbackTx => {
                let tx_manager = self.session_manager.take_session_tx(session_id)?;
                self.contract.worker_abort_tx_async(tx_manager).await?;
                Ok(())
            }
        }
    }

    pub(crate) async fn put_in_session_async(
        &self,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "worker_put_in_session_start");
        let handled = self
            .session_manager
            .with_session_tx(session_id, |tx_manager| match tx_manager {
                Some(tx_manager) => {
                    tx_manager.put(key.clone(), value.clone());
                    Ok(true)
                }
                None => Ok(false),
            })?;
        if handled {
            trace.watch("put.stage", "worker_put_in_session_staged_tx");
            Ok(())
        } else {
            trace.watch("put.stage", "worker_put_in_session_autocommit");
            self.contract.worker_put_async(key, value).await
        }
    }

    pub(crate) async fn delete_in_session_async(&self, session_id: OID, key: &[u8]) -> RS<()> {
        let key_vec = key.to_vec();
        let handled = self
            .session_manager
            .with_session_tx(session_id, |tx_manager| match tx_manager {
                Some(tx_manager) => {
                    tx_manager.delete(key_vec.clone());
                    Ok(true)
                }
                None => Ok(false),
            })?;
        if handled {
            Ok(())
        } else {
            self.contract.worker_delete_async(key).await
        }
    }

    pub(crate) async fn get_in_session(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        let tx_manager = self.session_manager.with_session_tx(session_id, Ok)?;
        let staged = tx_manager
            .as_ref()
            .and_then(|tx_manager| tx_manager.get(key));
        match staged {
            Some(value) => Ok(value),
            None => match tx_manager {
                Some(tx_manager) => {
                    self.contract
                        .worker_get_with_snapshot_async(&tx_manager.snapshot(), key)
                        .await
                }
                None => self.contract.worker_get_async(key).await,
            },
        }
    }

    pub(crate) async fn range_in_session(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        let tx_manager = self.session_manager.with_session_tx(session_id, Ok)?;
        let staged = tx_manager
            .as_ref()
            .map(|tx_manager| tx_manager.staged_items_in_range(start_key, end_key))
            .unwrap_or_default();

        let mut merged = BTreeMap::new();
        let base_items = match tx_manager {
            Some(tx_manager) => {
                self.contract
                    .worker_range_scan_with_snapshot_async(
                        &tx_manager.snapshot(),
                        start_key,
                        end_key,
                    )
                    .await?
            }
            None => {
                self.contract
                    .worker_range_scan_async(start_key, end_key)
                    .await?
            }
        };
        for item in base_items {
            merged.insert(item.key, Some(item.value));
        }
        for (key, value) in staged {
            merged.insert(key, value);
        }
        Ok(merged
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| KvItem { key, value }))
            .collect())
    }
}
