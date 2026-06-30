use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::wal::xl_data_op::{XLDelete, XLInsert, XLWrite};
use crate::wal::xl_entry::{TxOp, XLEntry};
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use mudu_utils::task_trace;
use std::cell::RefCell;
use std::collections::BTreeMap;
use tracing::trace;

struct WorkerTxState {
    stage_kv_write: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    staged_relation_ops: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    write_ops: Vec<(PhysicalRelationId, Vec<u8>)>,
    log_buffer: Vec<TxOp>,
    // Tracks the index of each key in log_buffer so that duplicate writes
    // within the same transaction replace the previous log entry instead of
    // appending a new one. Only the last write to a key is kept.
    kv_log_index: BTreeMap<Vec<u8>, usize>,
    relation_log_index: BTreeMap<(PhysicalRelationId, Vec<u8>), usize>,
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
                stage_kv_write: BTreeMap::new(),
                staged_relation_ops: BTreeMap::new(),
                write_ops: Vec::new(),
                log_buffer: Vec::new(),
                kv_log_index: BTreeMap::new(),
                relation_log_index: BTreeMap::new(),
            }),
        }
    }

    fn with_state<R>(&self, f: impl FnOnce(&WorkerTxState) -> R) -> R {
        #[expect(
            clippy::expect_used,
            reason = "reentrant borrow is a programming error"
        )]
        let state = self
            .state
            .try_borrow()
            .expect("worker tx manager state reentrant immutable borrow");
        f(&state)
    }

    fn with_state_mut<R>(&self, f: impl FnOnce(&mut WorkerTxState) -> R) -> R {
        #[expect(
            clippy::expect_used,
            reason = "reentrant borrow is a programming error"
        )]
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
            let op = TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: key.clone(),
                value: value.clone(),
            }));
            state.state_kv_write(key, Some(value), op);
        });
        trace.watch("tx.state.op", "put:done");
    }

    fn delete(&self, key: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "delete:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        self.with_state_mut(|state| {
            let op = TxOp::Write(XLWrite::Delete(XLDelete {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: key.clone(),
            }));
            state.state_kv_write(key, None, op);
        });
        trace.watch("tx.state.op", "delete:done");
    }

    fn get(&self, key: &[u8]) -> Option<Option<Vec<u8>>> {
        let trace = task_trace!();
        trace.watch("tx.state.op", "get:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        let result = self.with_state(|state| state.stage_kv_write.get(key).cloned());
        trace.watch("tx.state.op", "get:done");
        result
    }

    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "put_relation:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        trace.watch("tx.state.relation_id", &format!("{relation_id:?}"));
        self.with_state_mut(|state| {
            let op = TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: relation_id.table_id,
                partition_id: relation_id.partition_id,
                tuple_id: 0,
                key: key.clone(),
                value: value.clone(),
            }));
            let key = (relation_id, key);
            state.stage_rel_write(key, Some(value), op);
        });
        trace.watch("tx.state.op", "put_relation:done");
    }

    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>) {
        let trace = task_trace!();
        trace.watch("tx.state.op", "delete_relation:enter");
        trace.watch("tx.state.xid", &self.snapshot.xid().to_string());
        trace.watch("tx.state.relation_id", &format!("{relation_id:?}"));
        self.with_state_mut(|state| {
            let op = TxOp::Write(XLWrite::Delete(XLDelete {
                table_id: relation_id.table_id,
                partition_id: relation_id.partition_id,
                tuple_id: 0,
                key: key.clone(),
            }));
            let key = (relation_id, key.clone());
            state.stage_rel_write(key, None, op);
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
                .stage_kv_write
                .iter()
                .filter(|(key, _)| is_key_in_range(key, start_key, end_key))
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    }

    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        self.with_state(|state| state.stage_kv_write.clone())
    }

    fn is_empty(&self) -> bool {
        self.with_state(|state| {
            state.stage_kv_write.is_empty() && state.staged_relation_ops.is_empty()
        })
    }

    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        self.with_state(|state| state.write_ops.clone())
    }

    fn build_write_ops(&self) {
        self.with_state_mut(|state| {
            state.write_ops.clear();
            let mut write_ops = Vec::new();
            for key in state.stage_kv_write.keys() {
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

impl WorkerTxState {
    fn state_kv_write(&mut self, key: Vec<u8>, opt_value: Option<Vec<u8>>, op: TxOp) {
        match self.kv_log_index.get(&key) {
            Some(&idx) => self.log_buffer[idx] = op,
            None => {
                self.kv_log_index.insert(key.clone(), self.log_buffer.len());
                self.log_buffer.push(op);
            }
        }
        self.stage_kv_write.insert(key, opt_value);
    }

    fn stage_rel_write(
        &mut self,
        key: (PhysicalRelationId, Vec<u8>),
        opt_value: Option<Vec<u8>>,
        op: TxOp,
    ) {
        match self.relation_log_index.get(&key) {
            Some(&idx) => self.log_buffer[idx] = op,
            None => {
                self.relation_log_index
                    .insert(key.clone(), self.log_buffer.len());
                self.log_buffer.push(op);
            }
        }
        self.staged_relation_ops
            .entry(key.0)
            .or_default()
            .insert(key.1, opt_value);
    }
}
