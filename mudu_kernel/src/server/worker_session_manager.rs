use crate::contract::meta_mgr::MetaMgr;
use crate::mudu_conn::mudu_conn_core::MuduConnCore;
use crate::x_engine::tx_mgr::TxMgr;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::common::xid::new_xid;
use mudu::error::ec::EC;
use mudu::m_error;
use scc::HashMap as SccHashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

pub(crate) struct WorkerSessionManager {
    session_owner: SccHashMap<OID, u64>,
    connection_sessions: SccHashMap<u64, Arc<SccHashMap<OID, ()>>>,
    session_contexts: SccHashMap<OID, Arc<SessionContext>>,
    active_sessions: Arc<AtomicUsize>,
    meta_mgr: Arc<dyn MetaMgr>,
}

pub(crate) struct SessionContext {
    tx_manager: Mutex<Option<Arc<dyn TxMgr>>>,
    mudu_conn_core: Arc<MuduConnCore>,
}

impl WorkerSessionManager {
    pub(crate) fn new(active_sessions: Arc<AtomicUsize>, meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self {
            session_owner: SccHashMap::new(),
            connection_sessions: SccHashMap::new(),
            session_contexts: SccHashMap::new(),
            active_sessions,
            meta_mgr,
        }
    }

    pub(crate) fn create_session(&self, conn_id: u64) -> RS<OID> {
        loop {
            let session_id = new_xid();
            if self.session_owner.insert_sync(session_id, conn_id).is_err() {
                continue;
            }
            let session_context = Arc::new(SessionContext::new(self.meta_mgr.clone()));
            if self
                .session_contexts
                .insert_sync(session_id, session_context)
                .is_err()
            {
                let _ = self.session_owner.remove_sync(&session_id);
                continue;
            }
            let _ = self
                .connection_sessions(conn_id)
                .insert_sync(session_id, ());
            self.active_sessions.fetch_add(1, Ordering::Relaxed);
            return Ok(session_id);
        }
    }

    pub(crate) fn close_session(&self, conn_id: u64, session_id: OID) -> RS<bool> {
        match self
            .session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
        {
            Some(owner_conn_id) if owner_conn_id == conn_id => {
                let removed_owner = self.session_owner.remove_sync(&session_id).is_some();
                let _ = self.session_contexts.remove_sync(&session_id);
                if let Some(conn_sessions) = self.connection_sessions.get_sync(&conn_id) {
                    let conn_sessions = conn_sessions.get().clone();
                    let _ = conn_sessions.remove_sync(&session_id);
                }
                if removed_owner {
                    self.active_sessions.fetch_sub(1, Ordering::Relaxed);
                }
                Ok(true)
            }
            Some(_) => Err(m_error!(
                EC::TxErr,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Ok(false),
        }
    }

    pub(crate) fn close_connection_sessions(&self, conn_id: u64) -> RS<()> {
        if let Some((_conn_id, session_ids)) = self.connection_sessions.remove_sync(&conn_id) {
            session_ids.iter_sync(|session_id, _| {
                if self.session_owner.remove_sync(session_id).is_some() {
                    self.active_sessions.fetch_sub(1, Ordering::Relaxed);
                }
                let _ = self.session_contexts.remove_sync(session_id);
                true
            });
        }
        Ok(())
    }

    pub(crate) fn conn_id_for_session(&self, session_id: OID) -> RS<u64> {
        self.session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("session {} does not exist", session_id)
                )
            })
    }

    pub(crate) fn open_session(&self, session_id: OID) -> RS<OID> {
        let conn_id = self.conn_id_for_session(session_id)?;
        self.create_session(conn_id)
    }

    pub(crate) fn close_session_by_id(&self, session_id: OID) -> RS<()> {
        let conn_id = self.conn_id_for_session(session_id)?;
        let closed = self.close_session(conn_id, session_id)?;
        if closed {
            Ok(())
        } else {
            Err(m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            ))
        }
    }

    pub(crate) fn session_context(&self, session_id: OID) -> RS<Arc<SessionContext>> {
        self.session_contexts
            .get_sync(&session_id)
            .map(|entry| entry.get().clone())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("session {} does not exist", session_id)
                )
            })
    }

    pub(crate) fn ensure_session_owned_by_connection(
        &self,
        conn_id: u64,
        session_id: OID,
    ) -> RS<()> {
        match self
            .session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
        {
            Some(owner_conn_id) if owner_conn_id == conn_id => Ok(()),
            Some(_) => Err(m_error!(
                EC::TxErr,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Err(m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            )),
        }
    }

    pub(crate) fn adopt_connection_sessions(&self, conn_id: u64, session_ids: &[OID]) -> RS<()> {
        if session_ids.is_empty() {
            return Ok(());
        }
        let conn_sessions = self.connection_sessions(conn_id);
        for &session_id in session_ids {
            self.session_owner
                .insert_sync(session_id, conn_id)
                .map_err(|_| {
                    m_error!(
                        EC::ExistingSuchElement,
                        format!("session {} already exists on target worker", session_id)
                    )
                })?;
            if self
                .session_contexts
                .insert_sync(
                    session_id,
                    Arc::new(SessionContext::new(self.meta_mgr.clone())),
                )
                .is_err()
            {
                let _ = self.session_owner.remove_sync(&session_id);
                return Err(m_error!(
                    EC::ExistingSuchElement,
                    format!(
                        "session {} context already exists on target worker",
                        session_id
                    )
                ));
            }
            let _ = conn_sessions.insert_sync(session_id, ());
            self.active_sessions.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    pub(crate) fn connection_has_active_tx(&self, conn_id: u64) -> RS<bool> {
        let session_ids = self.connection_session_ids(conn_id);
        for session_id in session_ids {
            if self.has_session_tx(session_id)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(crate) fn has_session_tx(&self, session_id: OID) -> RS<bool> {
        Ok(self
            .session_context(session_id)?
            .tx_manager_cloned()
            .is_some())
    }

    pub(crate) fn begin_session_tx(&self, session_id: OID, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        let session = self.session_context(session_id)?;
        if session.tx_manager_cloned().is_some() {
            return Err(m_error!(
                EC::ExistingSuchElement,
                format!("session {} already has an active transaction", session_id)
            ));
        }
        session.set_tx_manager(Some(tx_mgr));
        Ok(())
    }

    pub(crate) fn take_session_tx(&self, session_id: OID) -> RS<Arc<dyn TxMgr>> {
        let session = self.session_context(session_id)?;
        session.take_tx_manager().ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("session {} has no active transaction", session_id)
            )
        })
    }

    pub(crate) fn with_session_tx<R, F>(&self, session_id: OID, f: F) -> RS<R>
    where
        F: FnOnce(Option<Arc<dyn TxMgr>>) -> RS<R>,
    {
        let session = self.session_context(session_id)?;
        f(session.tx_manager_cloned())
    }

    pub(crate) fn detach_connection_sessions(&self, conn_id: u64) -> RS<Vec<OID>> {
        let Some((_conn_id, conn_sessions)) = self.connection_sessions.remove_sync(&conn_id) else {
            return Ok(Vec::new());
        };
        let mut session_ids = Vec::new();
        conn_sessions.iter_sync(|session_id, _| {
            session_ids.push(*session_id);
            true
        });
        for &session_id in &session_ids {
            if self.session_owner.remove_sync(&session_id).is_some() {
                self.active_sessions.fetch_sub(1, Ordering::Relaxed);
            }
            let _ = self.session_contexts.remove_sync(&session_id);
        }
        Ok(session_ids)
    }

    fn connection_sessions(&self, conn_id: u64) -> Arc<SccHashMap<OID, ()>> {
        if let Some(existing) = self.connection_sessions.get_sync(&conn_id) {
            return existing.get().clone();
        }
        let created = Arc::new(SccHashMap::new());
        match self
            .connection_sessions
            .insert_sync(conn_id, created.clone())
        {
            Ok(_) => created,
            Err((_conn_id, created)) => {
                if let Some(existing) = self.connection_sessions.get_sync(&conn_id) {
                    existing.get().clone()
                } else {
                    created
                }
            }
        }
    }

    fn connection_session_ids(&self, conn_id: u64) -> Vec<OID> {
        let Some(conn_sessions) = self.connection_sessions.get_sync(&conn_id) else {
            return Vec::new();
        };
        let mut session_ids = Vec::new();
        conn_sessions.get().iter_sync(|session_id, _| {
            session_ids.push(*session_id);
            true
        });
        session_ids
    }
}

impl SessionContext {
    fn new(meta_mgr: Arc<dyn MetaMgr>) -> Self {
        Self {
            tx_manager: Mutex::new(None),
            mudu_conn_core: Arc::new(MuduConnCore::new(meta_mgr)),
        }
    }

    pub(crate) fn tx_manager_cloned(&self) -> Option<Arc<dyn TxMgr>> {
        self.tx_manager
            .lock()
            .expect("session tx manager lock poisoned")
            .clone()
    }

    pub(crate) fn set_tx_manager(&self, tx_manager: Option<Arc<dyn TxMgr>>) {
        *self
            .tx_manager
            .lock()
            .expect("session tx manager lock poisoned") = tx_manager;
    }

    pub(crate) fn take_tx_manager(&self) -> Option<Arc<dyn TxMgr>> {
        self.tx_manager
            .lock()
            .expect("session tx manager lock poisoned")
            .take()
    }

    pub(crate) fn mudu_conn_core(&self) -> Arc<MuduConnCore> {
        self.mudu_conn_core.clone()
    }
}
