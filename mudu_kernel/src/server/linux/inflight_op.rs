use mudu_sys::io::worker_ring::UserIoInflight;

pub(in crate::server) struct AcceptOp {
    addr: mudu_sys::uring::SockAddrBuf,
}

impl AcceptOp {
    pub(in crate::server) fn new(addr: mudu_sys::uring::SockAddrBuf) -> Self {
        Self { addr }
    }

    pub(in crate::server) fn addr(&self) -> &mudu_sys::uring::SockAddrBuf {
        &self.addr
    }

    pub(in crate::server) fn addr_mut(&mut self) -> &mut mudu_sys::uring::SockAddrBuf {
        &mut self.addr
    }
}

pub(in crate::server) enum InflightOp {
    Accept(Box<AcceptOp>),
    MailboxRead { _value: Box<u64> },
    UserIo(UserIoInflight),
}
