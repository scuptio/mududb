use ::catalog::contract::meta_mgr::MetaMgr;
use ::conn::mudu_conn_core::MuduConnCore;
use ::xengine::tx_mgr::TxMgr;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::SMutex;
use mudu_utils::oid::new_xid;
use scc::HashMap as SccHashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

/// Session registry of one worker: session ownership by connection, the
/// per-session execution contexts, and the live-session counter shared with
/// the contract layer.
pub struct WorkerSessionManager {
    session_owner: SccHashMap<OID, u64>,
    connection_sessions: SccHashMap<u64, Arc<SccHashMap<OID, ()>>>,
    session_contexts: SccHashMap<OID, Arc<SessionContext>>,
    active_sessions: Arc<AtomicUsize>,
    meta_mgr: Arc<dyn MetaMgr>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Clean up a session's still-open transaction on close (rolls it back,
    /// which releases its epoch registration — otherwise a dropped client
    /// connection with an open transaction pins the cluster epoch barrier
    /// forever).
    tx_cleanup: SessionTxCleanupSlot,
}

/// The session-close transaction cleanup hook slot.
type SessionTxCleanupSlot = SMutex<Option<SessionTxCleanup>>;

/// The session-close transaction cleanup hook.
type SessionTxCleanup = Arc<dyn Fn(Arc<dyn TxMgr>) + Send + Sync>;

pub(crate) struct SessionContext {
    tx_manager: SMutex<Option<Arc<dyn TxMgr>>>,
    mudu_conn_core: Arc<MuduConnCore>,
    is_admin: bool,
    /// Read-your-writes watermark (epoch design doc §5.2): the oracle
    /// current epoch captured at this session's last successful commit; a
    /// fresh session transaction waits it stable before registering.
    last_commit_epoch: AtomicU64,
}

impl WorkerSessionManager {
    /// Create an empty session manager; `active_sessions` is incremented for
    /// every open session and decremented on close.
    pub fn new(
        active_sessions: Arc<AtomicUsize>,
        meta_mgr: Arc<dyn MetaMgr>,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> Self {
        Self {
            session_owner: SccHashMap::new(),
            connection_sessions: SccHashMap::new(),
            session_contexts: SccHashMap::new(),
            active_sessions,
            meta_mgr,
            async_runtime,
            tx_cleanup: SMutex::new(None),
        }
    }

    /// Installs the transaction cleanup hook (the worker's contract aborts
    /// the tx).
    pub(crate) fn set_tx_cleanup(&self, cleanup: SessionTxCleanup) {
        if let Ok(mut slot) = self.tx_cleanup.lock() {
            *slot = Some(cleanup);
        }
    }

    /// Runs the cleanup hook over a session's open transaction, if any.
    fn cleanup_session_tx(&self, session: &SessionContext) {
        if let Ok(Some(tx)) = session.take_tx_manager() {
            if let Ok(Some(cleanup)) = self.tx_cleanup.lock().map(|slot| slot.clone()) {
                cleanup(tx);
            }
        }
    }

    pub(crate) fn create_session(&self, conn_id: u64) -> RS<OID> {
        self.create_session_with_admin(conn_id, false)
    }

    /// Create a session owned by connection `conn_id`; `is_admin` grants the
    /// session's connection core admin privileges.
    pub fn create_session_with_admin(&self, conn_id: u64, is_admin: bool) -> RS<OID> {
        loop {
            let session_id = new_xid();
            if self.session_owner.insert_sync(session_id, conn_id).is_err() {
                continue;
            }
            let session_context = Arc::new(SessionContext::new(
                self.meta_mgr.clone(),
                self.async_runtime.clone(),
                is_admin,
            )?);
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
                if let Some((_, session)) = self.session_contexts.remove_sync(&session_id) {
                    self.cleanup_session_tx(&session);
                }
                let removed_owner = self.session_owner.remove_sync(&session_id).is_some();
                if let Some(conn_sessions) = self.connection_sessions.get_sync(&conn_id) {
                    let conn_sessions = conn_sessions.get().clone();
                    let _ = conn_sessions.remove_sync(&session_id);
                }
                if removed_owner {
                    self.active_sessions.fetch_sub(1, Ordering::Relaxed);
                }
                Ok(true)
            }
            Some(_) => Err(mudu_error!(
                ErrorCode::Transaction,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Ok(false),
        }
    }

    /// Close every session owned by connection `conn_id`, returning the ids
    /// of the sessions that were closed.
    pub fn close_connection_sessions(&self, conn_id: u64) -> RS<Vec<OID>> {
        let mut closed = Vec::new();
        if let Some((_conn_id, session_ids)) = self.connection_sessions.remove_sync(&conn_id) {
            session_ids.iter_sync(|session_id, _| {
                if let Some((_, session)) = self.session_contexts.remove_sync(session_id) {
                    self.cleanup_session_tx(&session);
                }
                if self.session_owner.remove_sync(session_id).is_some() {
                    self.active_sessions.fetch_sub(1, Ordering::Relaxed);
                }
                closed.push(*session_id);
                true
            });
        }
        Ok(closed)
    }

    pub(crate) fn conn_id_for_session(&self, session_id: OID) -> RS<u64> {
        self.session_owner
            .get_sync(&session_id)
            .map(|entry| *entry.get())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
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
            Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("session {} does not exist", session_id)
            ))
        }
    }

    pub(crate) fn session_context(&self, session_id: OID) -> RS<Arc<SessionContext>> {
        self.session_contexts
            .get_sync(&session_id)
            .map(|entry| entry.get().clone())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("session {} does not exist", session_id)
                )
            })
    }

    /// Whether `session_id` was created with admin privileges.
    pub fn is_session_admin(&self, session_id: OID) -> RS<bool> {
        Ok(self.session_context(session_id)?.is_admin())
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
            Some(_) => Err(mudu_error!(
                ErrorCode::Transaction,
                format!(
                    "session {} does not belong to connection {}",
                    session_id, conn_id
                )
            )),
            None => Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("session {} does not exist", session_id)
            )),
        }
    }

    pub(crate) fn has_session_tx(&self, session_id: OID) -> RS<bool> {
        Ok(self
            .session_context(session_id)?
            .tx_manager_cloned()?
            .is_some())
    }

    pub(crate) fn begin_session_tx(&self, session_id: OID, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        let session = self.session_context(session_id)?;
        if session.tx_manager_cloned()?.is_some() {
            return Err(mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!("session {} already has an active transaction", session_id)
            ));
        }
        session.set_tx_manager(Some(tx_mgr))?;
        Ok(())
    }

    /// The session's read-your-writes watermark: the epoch its next
    /// transaction must wait stable before registering (0 = nothing
    /// committed yet).
    pub(crate) fn session_commit_epoch(&self, session_id: OID) -> RS<u64> {
        Ok(self
            .session_context(session_id)?
            .last_commit_epoch
            .load(Ordering::Relaxed))
    }

    /// Struct the epoch watermark of the session's successful commit.
    pub(crate) fn note_session_commit_epoch(&self, session_id: OID, epoch: u64) -> RS<()> {
        self.session_context(session_id)?
            .last_commit_epoch
            .fetch_max(epoch, Ordering::Relaxed);
        Ok(())
    }

    pub(crate) fn take_session_tx(&self, session_id: OID) -> RS<Arc<dyn TxMgr>> {
        let session = self.session_context(session_id)?;
        session.take_tx_manager()?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("session {} has no active transaction", session_id)
            )
        })
    }

    pub(crate) fn with_session_tx<R, F>(&self, session_id: OID, f: F) -> RS<R>
    where
        F: FnOnce(Option<Arc<dyn TxMgr>>) -> RS<R>,
    {
        let session = self.session_context(session_id)?;
        f(session.tx_manager_cloned()?)
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
}

impl SessionContext {
    fn new(
        meta_mgr: Arc<dyn MetaMgr>,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        is_admin: bool,
    ) -> RS<Self> {
        Ok(Self {
            tx_manager: SMutex::new(None),
            mudu_conn_core: Arc::new(MuduConnCore::new(meta_mgr, async_runtime, is_admin)?),
            is_admin,
            last_commit_epoch: AtomicU64::new(0),
        })
    }

    pub(crate) fn tx_manager_cloned(&self) -> RS<Option<Arc<dyn TxMgr>>> {
        Ok(self.tx_manager.lock()?.clone())
    }

    pub(crate) fn set_tx_manager(&self, tx_manager: Option<Arc<dyn TxMgr>>) -> RS<()> {
        *self.tx_manager.lock()? = tx_manager;
        Ok(())
    }

    pub(crate) fn take_tx_manager(&self) -> RS<Option<Arc<dyn TxMgr>>> {
        Ok(self.tx_manager.lock()?.take())
    }

    pub(crate) fn mudu_conn_core(&self) -> Arc<MuduConnCore> {
        self.mudu_conn_core.clone()
    }

    pub fn is_admin(&self) -> bool {
        self.is_admin
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use ::catalog_testing::test_meta_mgr::TestMetaMgr;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn create_session_with_admin_sets_admin_flag() {
        let manager = WorkerSessionManager::new(
            Arc::new(AtomicUsize::new(0)),
            Arc::new(TestMetaMgr::new()),
            None,
        );
        let plain = manager.create_session(1).unwrap();
        assert!(!manager.session_context(plain).unwrap().is_admin());
        let admin = manager.create_session_with_admin(1, true).unwrap();
        assert!(manager.session_context(admin).unwrap().is_admin());
    }
}
