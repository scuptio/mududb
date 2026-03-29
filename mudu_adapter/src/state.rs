use mudu::common::id::OID;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

pub fn next_session_id() -> OID {
    NEXT_SESSION_ID.fetch_add(1, Ordering::Relaxed) as OID
}
