#![allow(clippy::unwrap_used)]

use crate::contract::lsn::LSN;

#[test]
fn lsn_basic_operations() {
    let a: LSN = 10;
    let b: LSN = 20;
    assert_eq!(a + b, 30);
    assert!(b > a);
    assert_eq!(b - a, 10);
    assert_eq!(a as u64, 10);
}
