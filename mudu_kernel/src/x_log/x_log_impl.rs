use crate::contract::lsn::LSN;
use crate::contract::x_log::{OptAppend, XLog};
use crate::contract::xl_batch::XLBatch;
use crate::contract::xl_rec::XLRec;
use crate::x_log::lsn_allocator::LSNAllocator;
use crate::x_log::lsn_syncer::LSNSyncer;
use mudu::common::bc_enc::encode_binary;
use mudu::common::buf::Buf;
use mudu::common::result::RS;

use crate::contract::waiter::Waiter;
use async_trait::async_trait;
use mudu_utils::task_trace;
use std::sync::Arc;
use tokio::sync::mpsc::{Receiver, Sender};

pub struct XLogImpl {
    inner: Arc<XLogImplInner>,
}

struct XLogImplInner {
    lsn_allocator: LSNAllocator,
    lsn_syncer: LSNSyncer,
    sender: Vec<LogSender>,
}

#[derive(Clone)]
pub struct XLogImplFlusher {
    lsn: LSN,
    syncer: LSNSyncer,
}

impl XLogImpl {
    pub fn new(lsn_allocator: LSNAllocator, lsn_syncer: LSNSyncer, sender: Vec<LogSender>) -> Self {
        Self {
            inner: Arc::new(XLogImplInner::new(lsn_allocator, lsn_syncer, sender)),
        }
    }
}

impl XLogImplFlusher {
    fn new(lsn: LSN, syncer: LSNSyncer) -> XLogImplFlusher {
        Self { lsn, syncer }
    }
}

impl XLogImplInner {
    pub fn new(lsn_allocator: LSNAllocator, lsn_syncer: LSNSyncer, sender: Vec<LogSender>) -> Self {
        if sender.len() == 0 {
            panic!("log sender size cannot be 0")
        }
        Self {
            lsn_allocator,
            lsn_syncer,
            sender,
        }
    }

    async fn append_gut(
        &self,
        log_rec: Vec<XLRec>,
        wait: bool,
    ) -> RS<(LSN, Option<Arc<dyn Waiter<LSN>>>)> {
        let _trace = task_trace!();
        let lsn = self.lsn_allocator.next();
        let batch = XLBatch::new(lsn, log_rec);
        let buf = encode_binary(&batch).unwrap();
        let n = (lsn as usize) % self.sender.len();
        //info!("append log lsn {}", lsn);
        let r = self.sender[n].send((buf, lsn)).await;
        r.expect("append log error");
        let opt_flusher: Option<Arc<dyn Waiter<LSN>>> = if wait {
            let flusher = Self::new_flusher(self.lsn_syncer.clone(), lsn);
            Some(Arc::new(flusher))
        } else {
            None
        };
        Ok((lsn, opt_flusher))
    }

    fn flush_all_gut(&self) -> RS<XLogImplFlusher> {
        let lsn = self.lsn_allocator.alloc_max();
        let flusher = Self::new_flusher(self.lsn_syncer.clone(), lsn);
        Ok(flusher)
    }

    fn flush_gut(&self, lsn: LSN) -> RS<XLogImplFlusher> {
        let flusher = Self::new_flusher(self.lsn_syncer.clone(), lsn);
        Ok(flusher)
    }

    fn new_flusher(syncer: LSNSyncer, lsn: LSN) -> XLogImplFlusher {
        XLogImplFlusher::new(lsn, syncer)
    }
}

#[async_trait]
impl XLog for XLogImpl {
    async fn append(
        &self,
        log_rec: Vec<XLRec>,
        opt: OptAppend,
    ) -> RS<(LSN, Option<Arc<dyn Waiter<LSN>>>)> {
        let _trace = task_trace!();
        self.inner.append_gut(log_rec, opt.wait).await
    }

    async fn flush(&self, lsn: LSN) -> RS<Arc<dyn Waiter<LSN>>> {
        let _trace = task_trace!();
        let f = self.inner.flush_gut(lsn)?;
        Ok(Arc::new(f))
    }

    async fn flush_all(&self) -> RS<Arc<dyn Waiter<LSN>>> {
        let _trace = task_trace!();
        let f = self.inner.flush_all_gut()?;
        Ok(Arc::new(f))
    }
}

unsafe impl Send for XLogImpl {}
unsafe impl Sync for XLogImpl {}

#[async_trait]
impl Waiter<LSN> for XLogImplFlusher {
    async fn wait(&self) -> RS<LSN> {
        let _ = task_trace!();
        self.syncer.flush(self.lsn).await;
        Ok(self.lsn)
    }
}

unsafe impl Send for XLogImplFlusher {}
unsafe impl Sync for XLogImplFlusher {}

pub type LogSender = Sender<(Buf, LSN)>;
pub type LogReceiver = Receiver<(Buf, LSN)>;
