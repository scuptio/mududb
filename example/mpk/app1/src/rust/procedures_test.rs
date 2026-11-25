//! Tests for the three app1 procedures: the two pure echo procedures are
//! exercised directly; `proc_sys_call_mtp` runs against the in-process
//! standalone adapter with a per-test SQLite file.

use crate::rust::procedures;
use mududb::common::id::OID;
use mududb::sys::sync::SMutex;
use mududb::sys_interface::sync_api::mudu_open;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

// The standalone adapter keeps process-global connection state keyed by the
// MUDU_CONNECTION path, so tests serialize on one mutex and each picks a
// unique database file (proc_sys_call_mtp creates the wallets table without
// IF EXISTS).
static TEST_MUTEX: OnceLock<SMutex<()>> = OnceLock::new();

struct TestDb {
    path: PathBuf,
    session_id: OID,
}

impl TestDb {
    fn open() -> Self {
        let nanos = mududb::sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = mududb::sys::env_var::temp_dir().join(format!("app1-proc-test-{nanos}.db"));
        mududb::sys::env_var::set_var("MUDU_CONNECTION", &format!("sqlite://{}", path.display()));
        let session_id = mudu_open().unwrap();
        Self { path, session_id }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = mududb::sys_interface::sync_api::mudu_close(self.session_id);
        mududb::sys::env_var::remove_var("MUDU_CONNECTION");
        let _ = mududb::sys::fs::sync::remove_file(&self.path);
    }
}

#[test]
#[cfg_attr(miri, ignore)]
fn proc_mtp_echoes_arguments() {
    let (sum, text) = procedures::proc_mtp(7, 1, 2, "hello".to_string()).unwrap();
    assert_eq!(sum, 3);
    assert_eq!(text, "xid:7, a=1, b=2, c=hello");
}

#[test]
#[cfg_attr(miri, ignore)]
fn proc2_mtp_echoes_arguments() {
    let (sum, text) = procedures::proc2_mtp(9, 10, 20, "world".to_string()).unwrap();
    assert_eq!(sum, 30);
    assert_eq!(text, "xid:9, a=10, b=20, c=world");
}

#[test]
#[cfg_attr(miri, ignore)]
fn proc_sys_call_mtp_seeds_and_reads_wallets() {
    let _guard = TEST_MUTEX.get_or_init(|| SMutex::new(())).lock().unwrap();
    let db = TestDb::open();

    let (sum, text) =
        procedures::proc_sys_call_mtp(db.session_id, 1, 2, "run".to_string()).unwrap();
    assert_eq!(sum, 3);
    assert!(
        text.starts_with("xid:") && text.contains("a=1, b=2, c=run, result "),
        "unexpected result: {text}"
    );
    // Two seeded wallet rows render as two lines.
    assert_eq!(text.matches('\n').count(), 2, "unexpected result: {text}");
}
