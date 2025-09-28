use crate::contract::pst_op::{DeleteKV, InsertKV, PstOp, UpdateV};
use crate::contract::s_task::STask;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use rusqlite::{params, Connection, Statement, Transaction};
use std::str::FromStr;
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
        let mut guard = self.inner.lock().map_err(|_e| m_error!(ER::MutexError, ""))?;
        guard.run_flush()?;
        Ok(())
    }
}

struct Inner {
    receiver: Receiver<Vec<PstOp>>,
    connection: Connection,
}

impl Inner {
    fn new(path: String, receiver: Receiver<Vec<PstOp>>) -> RS<Inner> {
        let connection = Connection::open(path)
            .map_err(|e| m_error!(ER::IOErr, "open sqlite db error", e))?;
        Ok(Self {
            receiver,
            connection,
        })
    }

    fn create(&mut self) -> RS<()> {
        let ddl1 = r#"
            CREATE TABLE IF NOT EXISTS data (
                table_id TEXT ,
                tuple_id TEXT,
                ts_min INTEGER NOT NULL,
                ts_max INTEGER NOT NULL,
                tuple_key BLOB NOT NULL,
                tuple_value BLOB NOT NULL,
                PRIMARY KEY(table_id, tuple_id)
            );
            "#;
        let ddl2 = r#"
            CREATE TABLE IF NOT EXISTS delta (
                table_id TEXT ,
                tuple_id TEXT,
                ts_min INTEGER NOT NULL,
                ts_max INTEGER NOT NULL,
                tuple_delta BLOB NOT NULL,
                PRIMARY KEY(table_id, tuple_id, ts_min)
            );
            "#;
        let _ = self
            .connection
            .execute(ddl1, ())
            .map_err(|e| m_error!(ER::IOErr, "create table data error", e))?;

        let _ = self
            .connection
            .execute(ddl2, ())
            .map_err(|e| m_error!(ER::IOErr, "create table delta error", e))?;
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

            let ok = Self::write(&mut self.connection, vec_cmds)?;
            if !ok {
                // stopped
                break;
            }
        }
        self.connection
            .cache_flush()
            .map_err(|e| m_error!(ER::IOErr, "", e))?;
        Ok(())
    }

    fn write(connection: &mut Connection, cmds: Vec<PstOp>) -> RS<bool> {
        let tran = connection
            .transaction()
            .map_err(|e| m_error!(ER::IOErr, "start transaction error", e))?;

        let mut notify = vec![];
        let mut stop = None;
        {
            let mut stmt = Self::prepare_statement(&tran)?;
            for c in cmds {
                match c {
                    PstOp::InsertKV(insert_kv) => {
                        Self::insert_kv(&mut stmt.stmt_insert_kv, insert_kv)?;
                    }
                    PstOp::UpdateV(update_v) => {
                        Self::update_v(&mut stmt.stmt_update_v, update_v)?;
                    }
                    PstOp::DeleteKV(delete_kv) => {
                        Self::delete_kv(&mut stmt.stmt_delete_kv, delete_kv)?;
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
        }
        tran.commit()
            .map_err(|e| m_error!(ER::IOErr, "commit transaction error", e))?;
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

    fn insert_kv(stmt: &mut Statement, insert_kv: InsertKV) -> RS<()> {
        let table_id = oid_2_text(insert_kv.table_id);
        let tuple_id = oid_2_text(insert_kv.tuple_id);
        stmt.execute(params![
            table_id,
            tuple_id,
            insert_kv.timestamp.c_min(),
            insert_kv.timestamp.c_max(),
            insert_kv.key,
            insert_kv.value
        ])
            .map_err(|e| m_error!(ER::IOErr, "", e))?;
        Ok(())
    }

    fn update_v(stmt: &mut Statement, update_v: UpdateV) -> RS<()> {
        let table_id = oid_2_text(update_v.table_id);
        let tuple_id = oid_2_text(update_v.tuple_id);
        stmt.execute(params![
            update_v.timestamp.c_min(),
            update_v.timestamp.c_max(),
            update_v.value,
            table_id,
            tuple_id,
        ])
            .map_err(|e| m_error!(ER::IOErr, "", e))?;
        Ok(())
    }

    fn delete_kv(stmt: &mut Statement, delete_kv: DeleteKV) -> RS<()> {
        let table_id = oid_2_text(delete_kv.table_id);
        let tuple_id = oid_2_text(delete_kv.tuple_id);
        stmt.execute(params![table_id, tuple_id])
            .map_err(|e| m_error!(ER::IOErr, "", e))?;
        Ok(())
    }

    fn prepare_statement<'a>(tran: &'a Transaction<'a>) -> RS<SQLStmt<'a>> {
        let insert_kv = r#"
            INSERT INTO data (
                table_id, tuple_id,
                ts_min, ts_max,
                tuple_key, tuple_value
            ) VALUES (?, ?, ?, ?, ?, ?)"#;
        let update_value = r#"
            UPDATE data SET
                ts_min = ?,
                ts_max = ?,
                tuple_value = ?
            WHERE table_id = ? AND tuple_id = ?
            "#;
        let delete_kv = r#"
            DELETE FROM data WHERE table_id = ? AND tuple_id = ?
        "#;
        let stmt_insert_kv = tran
            .prepare(insert_kv)
            .map_err(|e| m_error!(ER::IOErr, "prepare put key value error", e))?;
        let stmt_update_v = tran
            .prepare(update_value)
            .map_err(|e| m_error!(ER::IOErr, "prepare update value error", e))?;
        let stmt_delete_kv = tran
            .prepare(delete_kv)
            .map_err(|e| m_error!(ER::IOErr, "prepare delete key value error", e))?;
        Ok(SQLStmt {
            stmt_insert_kv,
            stmt_update_v,
            stmt_delete_kv,
        })
    }
}

fn oid_2_text(oid: OID) -> String {
    oid.to_string()
}

fn text_2_oid(str: &str) -> RS<OID> {
    OID::from_str(str).map_err(|e| m_error!(ER::ParseErr, format!("parse string {} error", str), e))
}
struct SQLStmt<'c> {
    stmt_insert_kv: Statement<'c>,
    stmt_update_v: Statement<'c>,
    stmt_delete_kv: Statement<'c>,
}

impl STask for PstStoreImpl {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn run(self) -> RS<()> {
        self.run_flush()
    }
}
