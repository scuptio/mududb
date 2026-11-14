//! Message boundary for catalog relations used by the shared metadata service.
//! Data partitions keep direct local Relation access. Only shared catalog callers
//! use this handle; each catalog relation is created and dropped on its owner thread.
use super::relation::Relation;
use catalog::contract::{table_desc::TableDesc, worker_snapshot::WorkerSnapshot};
use futures::future::LocalBoxFuture;
use mudu::{
    common::{id::OID, result::RS},
    error::ErrorCode,
    mudu_error,
};
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use std::{ops::Bound, sync::Arc};
use tokio::sync::{mpsc, oneshot};

type Request = Box<dyn for<'a> FnOnce(&'a Relation) -> LocalBoxFuture<'a, ()> + Send>;

/// Sendable catalog mailbox. The owned paged index never crosses a thread boundary.
pub struct CatalogActor {
    sender: mpsc::Sender<Request>,
    table: OID,
    partition: OID,
}
fn stopped() -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Storage, "catalog relation owner stopped")
}
impl CatalogActor {
    /// Opens a catalog using the caller's I/O provider on a dedicated owner thread.
    pub async fn new(table: OID, partition: OID, path: String, desc: &TableDesc) -> RS<Self> {
        Self::new_with_provider(
            mudu_sys::default_sys_io_context().provider_arc(),
            table,
            partition,
            path,
            desc,
        )
        .await
    }
    /// Opens on a local executor; only requests and owned responses cross threads.
    pub async fn new_with_provider(
        provider: Arc<dyn AsyncIoProvider>,
        table: OID,
        partition: OID,
        path: String,
        desc: &TableDesc,
    ) -> RS<Self> {
        let desc = desc.clone();
        let (sender, mut receiver) = mpsc::channel::<Request>(64);
        let (ready, opened) = oneshot::channel();
        mudu_sys::task::sync::spawn_thread(move || {
            let _ = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                let relation = match Relation::new_with_provider(
                    provider, table, partition, path, &desc,
                )
                .await
                {
                    Ok(relation) => relation,
                    Err(error) => {
                        let _ = ready.send(Err(error));
                        return;
                    }
                };
                // Cancellation of open must not leave an orphan owner task.
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(request) = receiver.recv().await {
                    // Accepted operations finish even when the requester is cancelled.
                    request(&relation).await;
                }
            });
        })?;
        opened.await.map_err(|_| stopped())??;
        Ok(Self {
            sender,
            table,
            partition,
        })
    }
    async fn call<T: Send + 'static>(
        &self,
        operation: impl for<'a> FnOnce(&'a Relation) -> LocalBoxFuture<'a, RS<T>> + Send + 'static,
    ) -> RS<T> {
        let (reply, receive) = oneshot::channel();
        self.sender
            .send(Box::new(move |relation| {
                Box::pin(async move {
                    let _ = reply.send(operation(relation).await);
                })
            }))
            .await
            .map_err(|_| stopped())?;
        receive.await.map_err(|_| stopped())?
    }
    /// Returns the table identity without a round trip.
    pub fn table_id(&self) -> OID {
        self.table
    }
    /// Returns the partition identity without a round trip.
    pub fn partition_id(&self) -> OID {
        self.partition
    }
    /// Reads owned catalog rows using the supplied snapshot.
    pub async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let bounds = (bounds.0.map(<[u8]>::to_vec), bounds.1.map(<[u8]>::to_vec));
        let snapshot = snapshot.clone();
        self.call(move |relation| {
            Box::pin(async move {
                relation
                    .visible_range(
                        (
                            bounds.0.as_ref().map(Vec::as_slice),
                            bounds.1.as_ref().map(Vec::as_slice),
                        ),
                        &snapshot,
                    )
                    .await
            })
        })
        .await
    }
    /// Publishes a catalog value on the owner thread.
    pub async fn write_value(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        self.call(move |relation| Box::pin(relation.write_value(key, value, xid)))
            .await
    }
    /// Publishes a catalog tombstone on the owner thread.
    pub async fn write_delete(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        self.call(move |relation| Box::pin(relation.write_delete(key, xid)))
            .await
    }
    /// Drains catalog WAL writes before acknowledging durability.
    pub async fn flush_wal_async(&self) -> RS<()> {
        self.call(|relation| Box::pin(relation.flush_wal_async()))
            .await
    }
    /// Flushes dirty catalog pages on their owning executor.
    pub async fn flush_dirty_pages(&self) -> RS<()> {
        self.call(|relation| Box::pin(relation.flush_dirty_pages()))
            .await
    }
}
