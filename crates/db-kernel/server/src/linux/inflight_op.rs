use mudu_sys::io::worker_ring::UserIoInflight;

pub(crate) struct AcceptOp {
    addr: mudu_sys::io::iouring::SockAddrBuf,
}

impl AcceptOp {
    pub fn new(addr: mudu_sys::io::iouring::SockAddrBuf) -> Self {
        Self { addr }
    }

    pub(crate) fn addr(&self) -> &mudu_sys::io::iouring::SockAddrBuf {
        &self.addr
    }

    pub(crate) fn addr_mut(&mut self) -> &mut mudu_sys::io::iouring::SockAddrBuf {
        &mut self.addr
    }
}

pub(crate) enum InflightOp {
    Accept(Box<AcceptOp>),
    MailboxRead { _value: Box<u64> },
    UserIo(UserIoInflight),
}
