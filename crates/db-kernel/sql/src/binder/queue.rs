//! Queue statement binding methods of the `Binder` impl (RECEIVE/ACK and the
//! named consumer cursors).
use super::*;

impl Binder {
    /// Bind a queue `RECEIVE` statement: resolve the queue through the search
    /// path and build the envelope descriptor (`__seq`, `__lsn`, then the
    /// payload columns).
    pub(super) async fn bind_receive(&self, stmt: &StmtReceive) -> RS<BoundReceive> {
        let desc = self.resolve_queue(stmt.name()).await?;
        let count = stmt.count().unwrap_or(1);
        if count == 0 {
            return Err(mudu_error!(
                ER::InvalidArgument,
                "RECEIVE count must be at least 1"
            ));
        }
        Ok(BoundReceive {
            table_id: desc.id(),
            count,
            wait: stmt.wait(),
            consumer: stmt.consumer().map(str::to_string),
            tuple_desc: queue_envelope_tuple_desc(desc.as_ref()),
        })
    }

    /// Bind a queue `ACK` statement: resolve the queue through the search
    /// path.
    pub(super) async fn bind_ack(&self, stmt: &StmtAck) -> RS<BoundAck> {
        let desc = self.resolve_queue(stmt.name()).await?;
        Ok(BoundAck {
            table_id: desc.id(),
            seq: stmt.seq(),
            consumer: stmt.consumer().map(str::to_string),
        })
    }

    /// Bind a `CREATE CONSUMER` statement: resolve the queue the consumer
    /// cursor is registered on through the search path.
    pub(super) async fn bind_create_consumer(
        &self,
        stmt: &StmtCreateConsumer,
    ) -> RS<BoundCreateConsumer> {
        let desc = self.resolve_queue(stmt.queue()).await?;
        Ok(BoundCreateConsumer {
            table_id: desc.id(),
            consumer: stmt.consumer().to_string(),
        })
    }

    /// Bind a `DROP CONSUMER` statement: resolve the queue the consumer
    /// cursor belongs to through the search path.
    pub(super) async fn bind_drop_consumer(
        &self,
        stmt: &StmtDropConsumer,
    ) -> RS<BoundDropConsumer> {
        let desc = self.resolve_queue(stmt.queue()).await?;
        Ok(BoundDropConsumer {
            table_id: desc.id(),
            consumer: stmt.consumer().to_string(),
        })
    }

    async fn resolve_queue(&self, name: &str) -> RS<Arc<TableDesc>> {
        self.meta_mgr
            .get_queue(&self.current_schema, name)
            .await?
            .ok_or_else(|| mudu_error!(ER::EntityNotFound, format!("no such queue {}", name)))
    }
}

/// Whether `stmt` is a queue `RECEIVE` carrying a `WAIT`/`WAIT FOREVER`
/// clause. The worker SQL layer rejects such statements inside an explicit
/// transaction: WAIT parks the statement between short claim transactions,
/// which only autocommit mode can provide.
pub fn is_receive_wait_stmt(stmt: &StmtType) -> bool {
    matches!(
        stmt,
        StmtType::Command(StmtCommand::Receive(receive)) if receive.wait().is_some()
    )
}
