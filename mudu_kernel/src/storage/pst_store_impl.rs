use crate::contract::pst_op::{DeleteKV, InsertKV, PstOp, UpdateV};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::sync::s_task::STask;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

pub struct PstStoreImpl {
    name: String,
    inner: Arc<Mutex<Inner>>,
}

#[derive(Clone)]
pub struct PstOpChImpl {
    sender: Sender<Vec<PstOp>>,
}

impl PstOpChImpl {
    pub fn new(sender: Sender<Vec<PstOp>>) -> PstOpChImpl {
        Self { sender }
    }

    pub fn async_run_ops(&self, ops: Vec<PstOp>) -> RS<()> {
        self.sender
            .send(ops)
            .map_err(|e| m_error!(ER::IOErr, "", e))?;
        Ok(())
    }
}
impl PstStoreImpl {
    pub fn new(db_path: String, receiver: Receiver<Vec<PstOp>>) -> RS<PstStoreImpl> {
        let s = Self {
            name: "PST store flush".to_string(),
            inner: Arc::new(Mutex::new(Inner::new(db_path, receiver)?)),
        };
        Ok(s)
    }

    pub fn run_flush(&self) -> RS<()> {
        let mut guard = self
            .inner
            .lock()
            .map_err(|_e| m_error!(ER::MutexError, ""))?;
        guard.run_flush()?;
        Ok(())
    }
}

struct Inner {
    receiver: Receiver<Vec<PstOp>>,
    path: PathBuf,
    state: PersistedStore,
}

impl Inner {
    fn new(path: String, receiver: Receiver<Vec<PstOp>>) -> RS<Inner> {
        let path = PathBuf::from(path);
        let state = PersistedStore::load(&path)?;
        Ok(Self {
            receiver,
            path,
            state,
        })
    }

    fn create(&mut self) -> RS<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| m_error!(ER::IOErr, "create pst store dir error", e))?;
        }
        Ok(())
    }

    fn run_flush(&mut self) -> RS<()> {
        self.create()?;
        let channel = &self.receiver;
        loop {
            let mut vec_cmds = channel.recv().map_err(|e| m_error!(ER::IOErr, "", e))?;
            let try_iter = channel.try_iter();
            for c in try_iter {
                vec_cmds.extend(c);
            }

            let ok = Self::write(&self.path, &mut self.state, vec_cmds)?;
            if !ok {
                // stopped
                break;
            }
        }
        Ok(())
    }

    fn write(path: &Path, state: &mut PersistedStore, cmds: Vec<PstOp>) -> RS<bool> {
        let mut notify = vec![];
        let mut stop = None;
        for c in cmds {
            match c {
                PstOp::InsertKV(insert_kv) => {
                    Self::insert_kv(state, insert_kv);
                }
                PstOp::UpdateV(update_v) => {
                    Self::update_v(state, update_v)?;
                }
                PstOp::DeleteKV(delete_kv) => {
                    Self::delete_kv(state, delete_kv);
                }
                PstOp::WriteDelta(_) => {}
                PstOp::Flush(n) => {
                    notify.push(n);
                }
                PstOp::Stop(n) => {
                    stop = Some(n);
                    break;
                }
            }
        }
        state.save(path)?;
        for n in notify {
            let _ = n.send(());
        }
        match stop {
            None => {}
            Some(notify) => {
                let _ = notify.send(());
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn insert_kv(state: &mut PersistedStore, insert_kv: InsertKV) {
        let row_key = PersistedKey::new(insert_kv.table_id, insert_kv.tuple_id);
        let row = PersistedRow {
            table_id: oid_2_text(insert_kv.table_id),
            tuple_id: oid_2_text(insert_kv.tuple_id),
            ts_min: insert_kv.timestamp.c_min(),
            ts_max: insert_kv.timestamp.c_max(),
            tuple_key: insert_kv.key,
            tuple_value: insert_kv.value,
        };
        state.data.insert(row_key.as_map_key(), row);
    }

    fn update_v(state: &mut PersistedStore, update_v: UpdateV) -> RS<()> {
        let row_key = PersistedKey::new(update_v.table_id, update_v.tuple_id).as_map_key();
        let row = state
            .data
            .get_mut(&row_key)
            .ok_or_else(|| m_error!(ER::IOErr, "update missing pst row"))?;
        row.ts_min = update_v.timestamp.c_min();
        row.ts_max = update_v.timestamp.c_max();
        row.tuple_value = update_v.value;
        Ok(())
    }

    fn delete_kv(state: &mut PersistedStore, delete_kv: DeleteKV) {
        let row_key = PersistedKey::new(delete_kv.table_id, delete_kv.tuple_id).as_map_key();
        state.data.remove(&row_key);
    }
}

fn oid_2_text(oid: OID) -> String {
    oid.to_string()
}

#[derive(Default, Serialize, Deserialize)]
struct PersistedStore {
    data: BTreeMap<String, PersistedRow>,
}

impl PersistedStore {
    fn load(path: &Path) -> RS<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let text = fs::read_to_string(path)
            .map_err(|e| m_error!(ER::IOErr, "read pst store file error", e))?;
        serde_json::from_str(&text).map_err(|e| m_error!(ER::ParseErr, "parse pst store file error", e))
    }

    fn save(&self, path: &Path) -> RS<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| m_error!(ER::IOErr, "create pst store dir error", e))?;
        }
        let text = serde_json::to_string_pretty(self)
            .map_err(|e| m_error!(ER::ParseErr, "serialize pst store file error", e))?;
        let tmp_path = path.with_extension("json.tmp");
        fs::write(&tmp_path, text)
            .map_err(|e| m_error!(ER::IOErr, "write pst store temp file error", e))?;
        fs::rename(&tmp_path, path)
            .map_err(|e| m_error!(ER::IOErr, "replace pst store file error", e))?;
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct PersistedRow {
    table_id: String,
    tuple_id: String,
    ts_min: u64,
    ts_max: u64,
    tuple_key: Vec<u8>,
    tuple_value: Vec<u8>,
}

struct PersistedKey {
    table_id: OID,
    tuple_id: OID,
}

impl PersistedKey {
    fn new(table_id: OID, tuple_id: OID) -> Self {
        Self { table_id, tuple_id }
    }

    fn as_map_key(&self) -> String {
        format!("{}:{}", self.table_id, self.tuple_id)
    }
}

impl STask for PstStoreImpl {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn run(self) -> RS<()> {
        self.run_flush()
    }
}
