//! Unit tests for vote procedure input validation.

use crate::rust::procedure::create_vote;
use mududb::common::id::OID;

#[test]
fn create_vote_rejects_past_end_time() {
    let xid: OID = 0;
    let err = create_vote(
        xid,
        "creator".to_string(),
        "topic".to_string(),
        "single".to_string(),
        1,
        0,
        "always".to_string(),
    )
    .unwrap_err();
    assert!(err.message().contains("future"));
}

#[test]
fn create_vote_rejects_invalid_vote_type() {
    let xid: OID = 0;
    let future = mududb::sys::time::utc_now().timestamp() + 3600;
    let err = create_vote(
        xid,
        "creator".to_string(),
        "topic".to_string(),
        "ranked".to_string(),
        1,
        future,
        "always".to_string(),
    )
    .unwrap_err();
    assert!(err.message().contains("single") || err.message().contains("multiple"));
}

#[test]
fn create_vote_rejects_single_vote_with_multiple_choices() {
    let xid: OID = 0;
    let future = mududb::sys::time::utc_now().timestamp() + 3600;
    let err = create_vote(
        xid,
        "creator".to_string(),
        "topic".to_string(),
        "single".to_string(),
        3,
        future,
        "always".to_string(),
    )
    .unwrap_err();
    assert!(err.message().contains("Single vote"));
}

#[test]
fn create_vote_rejects_invalid_visibility_rule() {
    let xid: OID = 0;
    let future = mududb::sys::time::utc_now().timestamp() + 3600;
    let err = create_vote(
        xid,
        "creator".to_string(),
        "topic".to_string(),
        "multiple".to_string(),
        3,
        future,
        "secret".to_string(),
    )
    .unwrap_err();
    assert!(err.message().contains("Visibility"));
}
