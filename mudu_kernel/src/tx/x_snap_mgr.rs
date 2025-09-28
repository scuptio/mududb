use crate::contract::a_task::{ATask, AsyncTask};
use crate::contract::snapshot::{RunningXList, Snapshot};
use crate::sync::unique_inner::UniqueInner;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_utils::notifier::Notifier;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot::channel as oneshot_channel;
use tokio::sync::oneshot::{Receiver as OneshotReceiver, Sender as OneshotSender};

pub struct XSnapMgr {
    op_sender: OpSender,
    handler: AsyncTask,
}

#[derive(Clone)]
pub struct SnapshotRequester {
    op_sender: OpSender,
}

#[derive(Clone)]
pub struct XRunningHandler {
    inner: Arc<Mutex<_HandlerInner>>,
}

struct _HandlerInner {
    canceller: Notifier,
    name: String,
    channel: Mutex<Option<OpReceiver>>,
    ts: u64,
    running: Vec<u64>,
}

type OpSender = Sender<XListOp>;
type OpReceiver = Receiver<XListOp>;
type XListSender = OneshotSender<RunningXList>;
type XListReceiver = OneshotReceiver<RunningXList>;

enum XListOp {
    Add(XListSender),
    Remove(u64),
}

impl Debug for XListOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Add(_s) => f.write_str("Add"),
            Self::Remove(u) => f.write_fmt(format_args!("Remove:{:?}", u)),
        }
    }
}

impl XSnapMgr {
    pub fn new(canceller: Notifier, ts: u64, snap_request_queue_size: usize) -> Self {
        let (s, r) = channel::<XListOp>(snap_request_queue_size);

        let handler = _HandlerInner::new(canceller, "snap_alloc".to_string(), ts, r);
        Self {
            op_sender: s,
            handler: Arc::new(UniqueInner::new(handler)),
        }
    }

    pub fn snap_assign_task(&self) -> AsyncTask {
        self.handler.clone()
    }

    pub fn snapshot_requester(&self) -> SnapshotRequester {
        SnapshotRequester::new(self.op_sender.clone())
    }
}

impl SnapshotRequester {
    fn new(op_sender: OpSender) -> Self {
        Self { op_sender }
    }

    pub async fn start_tx(&self) -> RS<Snapshot> {
        let (s, r) = oneshot_channel::<RunningXList>();
        let op = XListOp::Add(s);
        let res = self.op_sender.send(op).await;
        res.map_err(|_e| m_error!(ER::MuduError, "", _e))?;
        let x_list = r.await.unwrap();
        let snapshot = Snapshot::from(x_list);
        Ok(snapshot)
    }

    pub async fn end_tx(&self, xid: XID) -> RS<()> {
        let xid = xid as u64;
        let op = XListOp::Remove(xid);
        let r = self.op_sender.send(op).await;
        r.map_err(|_e| m_error!(ER::MuduError, "", _e))?;
        Ok(())
    }
}

impl XRunningHandler {
    fn new(canceller: Notifier, ts: u64, channel: OpReceiver) -> Self {
        Self {
            inner: Arc::new(Mutex::new(_HandlerInner::new(
                canceller,
                "snap_mgr".to_string(),
                ts,
                channel,
            ))),
        }
    }

    pub async fn handle(&self) -> RS<()> {
        let mut inner = self.inner.lock().unwrap();
        inner.handle().await?;
        Ok(())
    }
}

impl _HandlerInner {
    fn new(canceller: Notifier, name: String, ts: u64, channel: OpReceiver) -> _HandlerInner {
        Self {
            canceller,
            name,
            channel: Mutex::new(Some(channel)),
            ts,
            running: vec![],
        }
    }

    async fn handle(&mut self) -> RS<()> {
        let mut channel = {
            let mut g_ch = self.channel.lock().unwrap();
            let mut opt_ch = None;
            std::mem::swap(&mut opt_ch, &mut g_ch);
            match opt_ch {
                Some(ch) => ch,
                None => {
                    panic!("cannot invoke handle multiple times")
                }
            }
        };
        loop {
            let limit = 10;
            let mut buf = Vec::with_capacity(limit);
            let _n = channel.recv_many(&mut buf, limit).await;
            for op in buf {
                match op {
                    XListOp::Add(s) => {
                        self.add_x(s);
                    }
                    XListOp::Remove(x) => self.remove_x(x),
                }
            }
        }
    }

    pub fn add_x(&mut self, senders: XListSender) {
        self.ts += 1;
        let x_list = RunningXList::new(self.ts, self.running.clone());
        senders.send(x_list).unwrap();
        self.add(self.ts);
    }

    pub fn remove_x(&mut self, ts: u64) {
        self.remove(ts);
    }

    pub fn add(&mut self, xid: u64) {
        let r = self.running.binary_search(&xid);
        match r {
            Ok(_index) => {}
            Err(index) => {
                self.running.insert(index, xid);
            }
        }
    }

    fn remove(&mut self, xid: u64) {
        let r = self.running.binary_search(&xid);
        if let Ok(index) = r {
            self.running.remove(index);
        }
    }
}

impl ATask for _HandlerInner {
    fn notifier(&self) -> Notifier {
        self.canceller.clone()
    }

    fn name(&self) -> String {
        self.name.clone()
    }

    async fn run(self) -> RS<()> {
        let mut s = self;
        s.handle().await
    }
}
