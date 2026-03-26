use crate::server_ur::worker_local_log::CloseFileRequest;
use crate::server_ur::worker_local_log::OpenFileRequest;

pub(in crate::server_ur) struct AcceptOp {
    addr: rliburing::sockaddr_storage,
    addr_len: rliburing::socklen_t,
}

impl AcceptOp {
    pub(in crate::server_ur) fn new(
        addr: rliburing::sockaddr_storage,
        addr_len: rliburing::socklen_t,
    ) -> Self {
        Self { addr, addr_len }
    }

    pub(in crate::server_ur) fn addr(&self) -> &rliburing::sockaddr_storage {
        &self.addr
    }

    pub(in crate::server_ur) fn addr_mut_ptr(&mut self) -> *mut rliburing::sockaddr {
        &mut self.addr as *mut _ as *mut rliburing::sockaddr
    }

    pub(in crate::server_ur) fn addr_len_mut(&mut self) -> *mut rliburing::socklen_t {
        &mut self.addr_len
    }

    pub(in crate::server_ur) fn addr_len(&self) -> rliburing::socklen_t {
        self.addr_len
    }
}

pub(in crate::server_ur) struct OpenFileOp {
    request: OpenFileRequest,
}

impl OpenFileOp {
    pub(in crate::server_ur) fn new(request: OpenFileRequest) -> Self {
        Self { request }
    }

    pub(in crate::server_ur) fn request_id(&self) -> u64 {
        self.request.request_id()
    }

    pub(in crate::server_ur) fn request(&self) -> &OpenFileRequest {
        &self.request
    }
}

pub(in crate::server_ur) struct CloseFileOp {
    request: CloseFileRequest,
}

impl CloseFileOp {
    pub(in crate::server_ur) fn new(request: CloseFileRequest) -> Self {
        Self { request }
    }

    pub(in crate::server_ur) fn request_id(&self) -> u64 {
        self.request.request_id()
    }

    pub(in crate::server_ur) fn request(&self) -> &CloseFileRequest {
        &self.request
    }
}

pub(in crate::server_ur) enum InflightOp {
    Accept(Box<AcceptOp>),
    MailboxRead { value: Box<u64> },
    Recv { conn_id: u64 },
    Send { conn_id: u64 },
    OpenFile(Box<OpenFileOp>),
    CloseFile(Box<CloseFileOp>),
    LogWrite,
    Close { conn_id: u64 },
}
