use crate::server_ur::routing::{ConnectionTransfer, SessionOpenTransferAction};
use mudu::common::id::OID;
use std::os::fd::RawFd;

#[derive(Debug)]
pub(in crate::server_ur) struct TransferredConnection {
    transfer: ConnectionTransfer,
    fd: RawFd,
    session_ids: Vec<OID>,
    session_open_action: Option<SessionOpenTransferAction>,
}

impl TransferredConnection {
    pub(in crate::server_ur) fn new(
        transfer: ConnectionTransfer,
        fd: RawFd,
        session_ids: Vec<OID>,
        session_open_action: Option<SessionOpenTransferAction>,
    ) -> Self {
        Self {
            transfer,
            fd,
            session_ids,
            session_open_action,
        }
    }

    pub(in crate::server_ur) fn transfer(&self) -> &ConnectionTransfer {
        &self.transfer
    }

    pub(in crate::server_ur) fn fd(&self) -> RawFd {
        self.fd
    }

    pub(in crate::server_ur) fn session_ids(&self) -> &[OID] {
        &self.session_ids
    }

    pub(in crate::server_ur) fn session_open_action(&self) -> Option<SessionOpenTransferAction> {
        self.session_open_action
    }
}
