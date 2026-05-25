use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::wal::xl_data_op::{XLDelete, XLInsert};
use crate::wal::xl_entry::{TxOp, XLEntry};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use mudu_utils::task_trace;
use std::cell::RefCell;
use std::collections::BTreeMap;
use tracing::trace;

struct WorkerTxState {
    staged_puts: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    staged_relation_ops: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    write_ops: Vec<(PhysicalRelationId, Vec<u8>)>,
    log_buffer: Vec<TxOp>,
}

pub struct WorkerTxManager {
    snapshot: WorkerSnapshot,
    state: RefCell<WorkerTxState>,
}

impl WorkerTxManager {
    pub fn new(snapshot: WorkerSnapshot) -> Self {
        Self {
            snapshot,
            state: RefCell::new(WorkerTxState {
                staged_puts: BTreeMap::new(),
                staged_relation_ops: BTreeMap::new(),
                write_ops: Vec::new(),
                log_buffer: Vec::new(),
            }),
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&WorkerTxState) -> R) -> R {
        let state = self
            .state
            .try_borrow()
            .expect("worker tx manager state reentrant immutable borrow");
        f(&state)
    }

    fn with_state_mut<R>(&self, f: impl FnOnce(&mut WorkerTxState) -> R) -> R {
        let mut state = self
            .state
            .try_borrow_mut()
            .expect("worker tx manager state reentrant mutable borrow");
        f(&mut state)
    }
}

unsafe impl Send for WorkerTxManager {}
unsafe impl Sync for WorkerTxManager {}

impl TxMgr for WorkerTxManager {
    fn xid(&self) -> u64 {
        self.snapshot.xid()
    }

    fn snapshot(&self) -> WorkerSnapshot {
        self.snapshot.clone()
    }

    fn put(&self, key: Vec<u8>, value: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "put:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        self.with_state_mut(|state| {
            state.staged_puts.insert(key.clone(), Some(value.clone()));
            state.log_buffer.push(TxOp::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key,
                value,
            }));
        });
        trace.watch("tx.state.op", "put:done");
    }

    fn delete(&self, key: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "delete:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        self.with_state_mut(|state| {
            state.staged_puts.insert(key.clone(), None);
            state.log_buffer.push(TxOp::Delete(XLDelete {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key,
            }));
        });
        trace.watch("tx.state.op", "delete:done");
    }

    fn get(&self, key: &[u8]) -> Option<Option<Vec<u8>>> {
        let trace = task_trace!();
        trace.watch("tx.state.op", "get:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        let result = self.with_state(|state| state.staged_puts.get(key).cloned());
        trace.watch("tx.state.op", "get:done");
        result
    }

    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "put_relation:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        trace.watch("tx.state.relation_id", &format!("{relation_id:?}"));
        self.with_state_mut(|state| {
            state
                .staged_relation_ops
                .entry(relation_id)
                .or_default()
                .insert(key.clone(), Some(value.clone()));
            state.log_buffer.push(TxOp::Insert(XLInsert {
                table_id: relation_id.table_id,
                partition_id: relation_id.partition_id,
                tuple_id: 0,
                key,
                value,
            }));
        });
        trace.watch("tx.state.op", "put_relation:done");
    }

    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "delete_relation:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        trace.watch("tx.state.relation_id", &format!("{relation_id:?}"));
        self.with_state_mut(|state| {
            state
                .staged_relation_ops
                .entry(relation_id)
                .or_default()
                .insert(key.clone(), None);
            state.log_buffer.push(TxOp::Delete(XLDelete {
                table_id: relation_id.table_id,
                partition_id: relation_id.partition_id,
                tuple_id: 0,
                key,
            }));
        });
        trace.watch("tx.state.op", "delete_relation:done");
    }

    fn get_relation(&self, relation_id: PhysicalRelationId, key: &[u8]) -> Option<Option<Vec<u8>>> {
        trace!(
            xid = self.snapshot.xid(),
            relation_id = ?relation_id,
            key_len = key.len(),
            "worker_tx_manager get_relation enter"
        );
        let result = self.with_state(|state| {
            state
                .staged_relation_ops
                .get(&relation_id)
                .and_then(|rows| rows.get(key).cloned())
        });
        trace!(
            xid = self.snapshot.xid(),
            relation_id = ?relation_id,
            found = result.is_some(),
            "worker_tx_manager get_relation exit"
        );
        result
    }

    fn staged_relation_items_in_range(
        &self,
        relation_id: PhysicalRelationId,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        self.with_state(|state| {
            state
                .staged_relation_ops
                .get(&relation_id)
                .map(|rows| {
                    rows.iter()
                        .filter(|(key, _)| is_key_in_range(key, start_key, end_key))
                        .map(|(key, value)| (key.clone(), value.clone()))
                        .collect()
                })
                .unwrap_or_default()
        })
    }

    fn staged_relation_ops(
        &self,
    ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>> {
        self.with_state(|state| state.staged_relation_ops.clone())
    }

    fn staged_items_in_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        self.with_state(|state| {
            state
                .staged_puts
                .iter()
                .filter(|(key, _)| is_key_in_range(key, start_key, end_key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    }

    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        self.with_state(|state| state.staged_puts.clone())
    }

    fn is_empty(&self) -> bool {
        self.with_state(|state| {
            state.staged_puts.is_empty() && state.staged_relation_ops.is_empty()
        })
    }

    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        self.with_state(|state| state.write_ops.clone())
    }

    fn build_write_ops(&self) {
        self.with_state_mut(|state| {
            state.write_ops.clear();
            let mut write_ops = Vec::new();
            for key in state.staged_puts.keys() {
                write_ops.push((
                    PhysicalRelationId {
                        table_id: 0,
                        partition_id: 0,
                    },
                    key.clone(),
                ));
            }
            for (relation_id, ops) in &state.staged_relation_ops {
                for key in ops.keys() {
                    write_ops.push((*relation_id, key.clone()));
                }
            }
            state.write_ops = write_ops;
            state.write_ops.sort();
        });
    }

    fn xl_batch(&self) -> XLBatch {
        self.with_state(|state| {
            let xid = self.snapshot.xid();
            let mut ops = Vec::with_capacity(state.log_buffer.len() + 2);
            ops.push(TxOp::Begin);
            ops.extend(state.log_buffer.clone());
            ops.push(TxOp::Commit);
            XLBatch::new(vec![XLEntry { xid, ops }])
        })
    }
}

fn is_key_in_range(key: &[u8], start_key: &[u8], end_key: &[u8]) -> bool {
    key >= start_key && (end_key.is_empty() || key < end_key)
}
