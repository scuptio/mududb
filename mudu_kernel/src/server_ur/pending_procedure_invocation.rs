use mudu::common::result::RS;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub struct PendingProcedureInvocation {
    conn_id: u64,
    request_id: u64,
    completed: Arc<AtomicBool>,
    future: Pin<Box<dyn Future<Output = RS<Vec<u8>>> + 'static>>,
}

impl PendingProcedureInvocation {
    pub fn new(
        conn_id: u64,
        request_id: u64,
        completed: Arc<AtomicBool>,
        future: Pin<Box<dyn Future<Output = RS<Vec<u8>>> + 'static>>,
    ) -> Self {
        Self {
            conn_id,
            request_id,
            completed,
            future,
        }
    }

    pub fn into_parts(
        self,
    ) -> (
        u64,
        u64,
        Arc<AtomicBool>,
        Pin<Box<dyn Future<Output = RS<Vec<u8>>> + 'static>>,
    ) {
        (self.conn_id, self.request_id, self.completed, self.future)
    }
}
