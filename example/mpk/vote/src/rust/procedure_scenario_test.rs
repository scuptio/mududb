//! Scenario tests for the vote procedures, run against the in-process
//! standalone adapter (SQLite). Complements `procedure_test.rs` (the
//! pure-validation rejection paths) with the database paths.

use crate::rust::procedure;
use mududb::common::id::OID;
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys::sync::SMutex;
use mududb::sys_interface::sync_api::{mudu_batch, mudu_close, mudu_command, mudu_open};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;

// The standalone adapter keeps process-global connection state keyed by the
// MUDU_CONNECTION path, so tests serialize on one mutex and each picks a
// unique database file.
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
        let path = mududb::sys::env_var::temp_dir().join(format!("vote-proc-test-{nanos}.db"));
        mududb::sys::env_var::set_var("MUDU_CONNECTION", &format!("sqlite://{}", path.display()));
        let session_id = mudu_open().unwrap();
        let ddl = include_str!("../../sql/ddl.sql");
        mudu_batch(session_id, sql_stmt!(&ddl), sql_params!(&())).unwrap();
        Self { path, session_id }
    }

    /// Inserts an already-ended vote row directly (create_vote rejects past
    /// end times), returning the vote id.
    fn insert_expired_vote(&self, visibility_rule: &str) -> String {
        let vote_id = mududb::sys::random::next_uuid_v4_string();
        let past = mududb::sys::time::utc_now().timestamp() - 3600;
        mudu_command(
            self.session_id,
            sql_stmt!(
                &"INSERT INTO votes (vote_id, creator_id, topic, vote_type, max_choices, end_time, visibility_rule)
                   VALUES (?, ?, ?, ?, ?, ?, ?)"
            ),
            sql_params!(&(
                vote_id.clone(),
                "creator".to_string(),
                "topic".to_string(),
                "single".to_string(),
                1i64,
                past,
                visibility_rule.to_string()
            )),
        )
        .unwrap();
        vote_id
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let _ = mudu_close(self.session_id);
        mududb::sys::env_var::remove_var("MUDU_CONNECTION");
        let _ = mududb::sys::fs::sync::remove_file(&self.path);
    }
}

fn lock() -> mududb::sys::sync::SMutexGuard<'static, ()> {
    TEST_MUTEX.get_or_init(|| SMutex::new(())).lock().unwrap()
}

fn future_end() -> i64 {
    mududb::sys::time::utc_now().timestamp() + 3600
}

#[test]
#[cfg_attr(miri, ignore)]
fn vote_full_lifecycle() {
    let _guard = lock();
    let db = TestDb::open();
    let xid = db.session_id;

    let user = procedure::create_user(xid, "12345678901".to_string()).unwrap();
    let vote = procedure::create_vote(
        xid,
        user.clone(),
        "Best language".to_string(),
        "single".to_string(),
        1,
        future_end(),
        "always".to_string(),
    )
    .unwrap();
    let opt_a = procedure::add_option(xid, vote.clone(), "Rust".to_string()).unwrap();
    let _opt_b = procedure::add_option(xid, vote.clone(), "Lean".to_string()).unwrap();

    // Cast, then read the result and history.
    procedure::cast_vote(xid, user.clone(), vote.clone(), vec![opt_a]).unwrap();
    let result = procedure::get_vote_result(xid, vote.clone()).unwrap();
    assert_eq!(result.total_votes, Some(1));
    assert_eq!(result.vote_ended, Some(0));
    let history = procedure::get_voting_history(xid, user.clone()).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].vote_id.as_deref(), Some(vote.as_str()));
    assert_eq!(history[0].is_withdrawn, Some(0));

    // Double voting is rejected while the first vote is active.
    let err =
        procedure::cast_vote(xid, user.clone(), vote.clone(), vec!["x".to_string()]).unwrap_err();
    assert!(err.to_string().contains("already voted"));

    // Withdraw, then the result empties and re-voting works.
    procedure::withdraw_vote(xid, user.clone(), vote.clone()).unwrap();
    let result = procedure::get_vote_result(xid, vote.clone()).unwrap();
    assert_eq!(result.total_votes, Some(0));
    let history = procedure::get_voting_history(xid, user.clone()).unwrap();
    assert_eq!(history[0].is_withdrawn, Some(1));
}

#[test]
#[cfg_attr(miri, ignore)]
fn vote_error_paths() {
    let _guard = lock();
    let db = TestDb::open();
    let xid = db.session_id;
    let missing = mududb::sys::random::next_uuid_v4_string();

    // Missing vote rows are reported as not found.
    let err = procedure::cast_vote(xid, "u".to_string(), missing.clone(), vec![]).unwrap_err();
    assert!(err.to_string().contains("Vote not found"));
    let err = procedure::withdraw_vote(xid, "u".to_string(), missing.clone()).unwrap_err();
    assert!(err.to_string().contains("Vote not found"));
    let err = procedure::get_vote_result(xid, missing).unwrap_err();
    assert!(err.to_string().contains("Vote not found"));

    // Choice-count validation.
    let vote = procedure::create_vote(
        xid,
        "creator".to_string(),
        "t".to_string(),
        "single".to_string(),
        1,
        future_end(),
        "always".to_string(),
    )
    .unwrap();
    let err = procedure::cast_vote(
        xid,
        "u".to_string(),
        vote.clone(),
        vec!["a".to_string(), "b".to_string()],
    )
    .unwrap_err();
    assert!(err.to_string().contains("exactly one option"));

    let multi = procedure::create_vote(
        xid,
        "creator".to_string(),
        "t".to_string(),
        "multiple".to_string(),
        3,
        future_end(),
        "always".to_string(),
    )
    .unwrap();
    let err = procedure::cast_vote(
        xid,
        "u".to_string(),
        multi,
        vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ],
    )
    .unwrap_err();
    assert!(err.to_string().contains("Exceeded max choices"));

    // Withdrawing without an active vote.
    let err = procedure::withdraw_vote(xid, "u".to_string(), vote).unwrap_err();
    assert!(err.to_string().contains("No active vote to withdraw"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn vote_ended_and_visibility_paths() {
    let _guard = lock();
    let db = TestDb::open();
    let xid = db.session_id;

    // An ended vote rejects casting and withdrawing.
    let ended = db.insert_expired_vote("always");
    let err = procedure::cast_vote(xid, "u".to_string(), ended.clone(), vec![]).unwrap_err();
    assert!(err.to_string().contains("Voting has ended"));
    let err = procedure::withdraw_vote(xid, "u".to_string(), ended.clone()).unwrap_err();
    assert!(err.to_string().contains("Voting has ended"));
    // Ended votes with "always" visibility still report results.
    let result = procedure::get_vote_result(xid, ended).unwrap();
    assert_eq!(result.vote_ended, Some(1));
    assert_eq!(result.total_votes, Some(0));

    // A live "after_end" vote hides its results until it ends.
    let hidden = procedure::create_vote(
        xid,
        "creator".to_string(),
        "t".to_string(),
        "single".to_string(),
        1,
        future_end(),
        "after_end".to_string(),
    )
    .unwrap();
    let err = procedure::get_vote_result(xid, hidden).unwrap_err();
    assert!(err.to_string().contains("only visible after vote ends"));

    // An ended "after_end" vote reports results.
    let ended_hidden = db.insert_expired_vote("after_end");
    let result = procedure::get_vote_result(xid, ended_hidden).unwrap();
    assert_eq!(result.vote_ended, Some(1));
}
