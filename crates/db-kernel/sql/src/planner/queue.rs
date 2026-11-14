//! Queue statement planning methods of the `Planner` impl (queue DDL,
//! consumer cursors, RECEIVE, ACK).

use super::*;

impl Planner {
    pub(super) fn plan_create_queue(&self, stmt: BoundCreateQueue) -> CreateQueue {
        CreateQueue::new(
            PCreateQueue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                schema: stmt.schema,
                partition_binding: stmt.partition_binding,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    pub(super) fn plan_drop_queue(&self, stmt: BoundDropQueue) -> DropQueue {
        DropQueue::new(
            PDropQueue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                oid: stmt.oid,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    pub(super) fn plan_create_consumer(&self, stmt: BoundCreateConsumer) -> CreateConsumer {
        CreateConsumer::new(
            PCreateConsumer {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                consumer: stmt.consumer,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    pub(super) fn plan_drop_consumer(&self, stmt: BoundDropConsumer) -> DropConsumer {
        DropConsumer::new(
            PDropConsumer {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                consumer: stmt.consumer,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    pub(super) fn plan_receive(&self, stmt: BoundReceive) -> ReceiveQueue {
        ReceiveQueue::new(
            PReceiveQueue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                count: stmt.count,
                wait: stmt.wait.map(|wait| match wait {
                    ReceiveWait::Timeout(ms) => QueueWait::Timeout(ms),
                    ReceiveWait::Forever => QueueWait::Forever,
                }),
                consumer: stmt.consumer,
                tuple_desc: stmt.tuple_desc,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }

    pub(super) fn plan_ack(&self, stmt: BoundAck) -> AckQueue {
        AckQueue::new(
            PAckQueue {
                tx_mgr: self.ctx.tx_mgr.clone(),
                table_id: stmt.table_id,
                seq: stmt.seq,
                consumer: stmt.consumer,
            },
            self.ctx.x_contract.clone(),
            self.ctx.meta_mgr.clone(),
        )
    }
}
