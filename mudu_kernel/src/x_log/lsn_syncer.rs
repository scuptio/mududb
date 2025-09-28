use crate::contract::lsn::LSN;
use lazy_static::lazy_static;
use mudu::common::_debug::enable_debug;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_utils::debug::register_debug_url;

use mudu_utils::task_trace;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::collections::HashMap;
use std::fmt::{Debug, Formatter, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub struct LSNSyncer {
    inner: Arc<LSNSyncerInner>,
}

lazy_static! {
    static ref _LSN_SYNCER: StdMutex<HashMap<OID, Arc<LSNSyncerInner>>> =
        StdMutex::new(HashMap::new());
}

struct LSNWait {
    waiting_list: Vec<(LSN, Arc<Notify>)>,
}

#[derive(Debug, Clone)]
struct LSNReadyInfo {
    max_flushed: LSN,
    min_ready: LSN,
    opt_flush_ready: Option<LSN>,
    ready_lsn: Vec<Vec<LSN>>,
}

struct LSNSyncerInner {
    id: OID,
    max_flushed_lsn: AtomicU64,
    ready_info: StdMutex<LSNReadyInfo>,
    lsn_wait: StdMutex<LSNWait>,
}

impl LSNSyncer {
    pub fn new(oid: OID) -> Self {
        Self {
            inner: Arc::new(LSNSyncerInner::new(oid)),
        }
    }

    pub fn recovery(&self, last_lsn: LSN) -> RS<()> {
        if enable_debug() {
            let mut _map = _LSN_SYNCER.lock().unwrap();
            let _ = _map.insert(self.inner.id(), self.inner.clone());
            register_debug_url("/lsn_syncer".to_string(), _debug_lsn_syncer);
        }
        self.inner.recovery(last_lsn)
    }

    pub fn finalize(&self) {
        if enable_debug() {
            let mut _map = _LSN_SYNCER.lock().unwrap();
            let _ = _map.remove(&self.inner.id());
        }
    }
    pub async fn flush(&self, lsn: LSN) {
        let _trace = task_trace!();
        self.inner.wait_flush(lsn).await
    }

    pub fn ready(&self, lsn: Vec<LSN>) {
        let _trace = task_trace!();
        self.inner.add_ready_lsn(lsn);
    }
}

impl Debug for LSNSyncerInner {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let max_flushed_lsn = self.max_flushed_lsn.load(Ordering::Relaxed);
        f.write_fmt(format_args!("max_flushed_lsn:{}\n", max_flushed_lsn))?;
        let ready_info = self.ready_info.try_lock().map_err(|_| std::fmt::Error)?;
        f.write_fmt(format_args!("ready_info:{:?}\n", &*ready_info))?;
        let lsn_wait = self.lsn_wait.try_lock().map_err(|_| std::fmt::Error)?;
        f.write_fmt(format_args!("lsn_wait:{:?}\n", &*lsn_wait))?;

        Ok(())
    }
}

impl LSNWait {
    fn new() -> Self {
        Self {
            waiting_list: vec![],
        }
    }

    fn add_new_wait(&mut self, lsn: LSN) -> Option<Arc<Notify>> {
        let notify: Arc<Notify> = Arc::new(Notify::new());
        self.waiting_list.push((lsn, notify.clone()));
        Some(notify)
    }

    fn update_notify(&mut self, max_lsn: LSN) {
        let mut result = vec![];
        let mut waiting_list = vec![];
        std::mem::swap(&mut waiting_list, &mut self.waiting_list);
        //info!("waiting list {:?}", waiting_list);
        let pair = (&mut result, &mut self.waiting_list);
        for (lsn, notify) in waiting_list {
            if lsn <= max_lsn {
                // to return result
                pair.0.push((lsn, notify));
            } else {
                // new self.waiting_list
                pair.1.push((lsn, notify));
            }
        }
        for (_n, notify) in pair.0 {
            //debug!("notify {}", _n);
            notify.notify_waiters();
        }
    }
}

impl Debug for LSNWait {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str("waiting_list")?;
        f.write_str("\t[")?;
        for (lsn, _) in self.waiting_list.iter() {
            f.write_fmt(format_args!("{} ", lsn))?;
        }
        f.write_str("]\n")?;
        Ok(())
    }
}
impl LSNSyncerInner {
    pub fn recovery(&self, last_lsn: LSN) -> RS<()> {
        self.max_flushed_lsn.store(last_lsn, Ordering::SeqCst);
        let mut info = self.ready_info.lock().unwrap();
        info.max_flushed = last_lsn;
        info.min_ready = LSN::MAX;
        Ok(())
    }

    fn new(id: OID) -> LSNSyncerInner {
        Self {
            id,
            max_flushed_lsn: AtomicU64::new(0),
            ready_info: StdMutex::new(LSNReadyInfo {
                max_flushed: 0,
                min_ready: LSN::MAX,
                opt_flush_ready: None,
                ready_lsn: vec![],
            }),
            lsn_wait: StdMutex::new(LSNWait::new()),
        }
    }

    fn id(&self) -> OID {
        self.id
    }

    async fn wait_flush(&self, lsn: LSN) {
        let _trace = task_trace!();
        let last_lsn = self.max_flushed_lsn.load(Ordering::SeqCst);
        if last_lsn < lsn {
            let opt_notify = {
                let mut _g = self.lsn_wait.lock().unwrap();
                _g.add_new_wait(lsn)
            };
            if let Some(notify) = opt_notify {
                let last_lsn = self.max_flushed_lsn.load(Ordering::SeqCst);
                if last_lsn < lsn {
                    notify.notified().await;
                }
            }
        }
    }

    fn notify_waiter(&self, lsn: LSN) {
        let _trace = task_trace!();
        let last_lsn = self.max_flushed_lsn.load(Ordering::SeqCst);
        if last_lsn < lsn {
            let mut g = self.lsn_wait.lock().unwrap();
            //info!("notify all lsn <= {}", lsn);
            g.update_notify(lsn);
            self.max_flushed_lsn.store(lsn, Ordering::SeqCst);
        }
    }

    fn update_lsn_ready_info(
        &self,
        _min_flushed: LSN,
        max_ready: LSN,
        lsn_vec: Vec<LSN>,
    ) -> Option<(LSN, Vec<Vec<LSN>>)> {
        let _trace = task_trace!();
        let mut info = self.ready_info.lock().unwrap();
        if !lsn_vec.is_empty() {
            info.ready_lsn.push(lsn_vec);
        }
        if info.max_flushed < max_ready {
            if info.opt_flush_ready.is_none() {
                panic!(
                    "cannot possible, ready info {:?}, max_flushed:{}",
                    info, max_ready
                );
            }
            let mut min_ready = LSN::MAX;
            for vec in info.ready_lsn.iter() {
                if let Some(l) = vec.first() {
                    if *l < min_ready {
                        min_ready = *l;
                    }
                    assert!(*l > max_ready);
                }
            }
            info.max_flushed = max_ready;
            info.min_ready = min_ready;
            info.opt_flush_ready = None;
            if info.max_flushed + 1 == info.min_ready {
                info.opt_flush_ready = Some(info.min_ready);
                let mut ready = vec![];
                std::mem::swap(&mut ready, &mut info.ready_lsn);
                Some((info.min_ready, ready))
            } else {
                None
            }
        } else {
            None
        }
    }

    fn notify_ready(&self, min_ready: LSN, ready: Vec<Vec<LSN>>) {
        let _trace = task_trace!();
        let (lsn_contiguous_vec, lsn_vec_new) = merge_sorted::<true>(ready);
        if let Some(lsn) = lsn_contiguous_vec.last() {
            let max_flushed = *lsn;
            self.notify_waiter(max_flushed);
            let opt_new_ready = self.update_lsn_ready_info(min_ready, max_flushed, lsn_vec_new);
            if let Some((min, new_ready)) = opt_new_ready {
                self.notify_ready(min, new_ready);
            }
        } else {
            assert!(lsn_contiguous_vec.is_empty());
            self.add_ready_lsn(lsn_vec_new);
        }
    }

    fn add_ready_lsn(&self, lsn_vec: Vec<LSN>) {
        let _trace = task_trace!();
        let mut lsn_vec = lsn_vec;
        lsn_vec.sort();
        let first_lsn = match lsn_vec.first() {
            Some(lsn) => *lsn,
            None => {
                return;
            }
        };
        let opt_ready = {
            let mut lsn_bound = self.ready_info.lock().unwrap();
            lsn_bound.ready_lsn.push(lsn_vec);
            if lsn_bound.max_flushed < first_lsn && lsn_bound.min_ready > first_lsn {
                lsn_bound.min_ready = first_lsn
            }
            if lsn_bound.max_flushed + 1 == lsn_bound.min_ready
                && lsn_bound.opt_flush_ready.is_none()
            {
                lsn_bound.opt_flush_ready = Some(lsn_bound.min_ready);
                let mut ready = vec![];
                std::mem::swap(&mut lsn_bound.ready_lsn, &mut ready);
                Some((lsn_bound.min_ready, ready))
            } else {
                None
            }
        };

        if let Some((min, ready)) = opt_ready {
            self.notify_ready(min, ready);
        }
    }
}

fn merge_sorted<const SPLIT_NON_CONTINUOUS: bool>(to_merge: Vec<Vec<LSN>>) -> (Vec<LSN>, Vec<LSN>) {
    let mut heap = BinaryHeap::new();
    let mut result = (Vec::new(), Vec::new());

    // initialize heap
    // put the first element of each Vec to the heap
    for (i, vec) in to_merge.iter().enumerate() {
        if let Some(&value) = vec.first() {
            heap.push(Reverse((value, i, 0))); // (item value，vec index，item position)
        }
    }

    let mut push_to_first = true;
    let mut prev_value = None;
    // peek the minimum item, and insert it to the heap
    while let Some(Reverse((value, i, j))) = heap.pop() {
        if SPLIT_NON_CONTINUOUS {
            if push_to_first {
                push_to_first = match prev_value {
                    Some(v) => v + 1 == value,
                    None => true,
                };

                prev_value = Some(value);
                if push_to_first {
                    result.0.push(value);
                } else {
                    result.1.push(value);
                }
            } else {
                result.1.push(value);
            }
        } else {
            result.0.push(value);
        }
        // if there are some items, put the items to heap
        if let Some(&next_value) = to_merge[i].get(j + 1) {
            heap.push(Reverse((next_value, i, j + 1)));
        }
    }

    result
}

fn _debug_lsn_syncer(_path: String) -> RS<String> {
    let _map = _LSN_SYNCER.lock().unwrap();
    let mut s = String::new();
    for (_, syncer) in _map.iter() {
        s.write_str(format!("{:?}", syncer).as_str()).unwrap()
    }
    Ok(s)
}

#[test]
fn test_merge_sorted() {
    let sorted_vec = vec![vec![1, 9], vec![2, 4, 6], vec![3, 7, 8]];

    let merged = merge_sorted::<true>(sorted_vec);
    println!("{:?}", merged);
}
