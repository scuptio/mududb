//! Regression checks connecting the pure sequence proofs to encoded pages.
#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::page_block_ref::PAGE_SIZE;
use super::page_block_ref_mut::PageBlockRefMut;
use std::collections::BTreeMap;

#[test]
fn mixed_page_edits_match_the_logical_record_map() {
    let mut bytes = [0u8; PAGE_SIZE];
    let mut page = PageBlockRefMut::new(&mut bytes);
    page.init_empty(1u64.into()).unwrap();
    let mut expected = BTreeMap::new();
    for key in (0..24u64).rev() {
        let payload = vec![key as u8; key as usize + 1];
        page.insert_record(key, key + 100, &payload).unwrap();
        expected.insert((key, key + 100), payload);
    }
    for turn in 0..24u64 {
        let index = (turn as usize * 7) % expected.len();
        let key = *expected.keys().nth(index).unwrap();
        if turn % 3 == 0 {
            page.delete_record(index).unwrap();
            expected.remove(&key);
        } else {
            let payload = vec![turn as u8; turn as usize + 5];
            page.update_record(index, key.0, key.1, &payload).unwrap();
            expected.insert(key, payload);
        }
        page.compact().unwrap();
        for (index, (key, payload)) in expected.iter().enumerate() {
            let slot = page.slot(index).unwrap();
            assert_eq!((slot.timestamp(), slot.tuple_id()), *key);
            assert_eq!(page.record_bytes(index).unwrap(), payload);
            assert_eq!(slot.offset() % 8, 0);
        }
        assert_eq!(
            page.header().unwrap().record_count() as usize,
            expected.len()
        );
    }
}

#[test]
fn every_failed_public_edit_preserves_all_page_bytes() {
    let mut bytes = [0u8; PAGE_SIZE + 16];
    let mut page = PageBlockRefMut::new(&mut bytes);
    page.init_empty(1u64.into()).unwrap();
    page.insert_record(1, 1, b"original").unwrap();
    let original = page.page().to_vec();
    assert!(page.insert_record(2, 2, &[7; PAGE_SIZE]).is_err());
    assert_eq!(page.page(), original);
    assert!(page.update_record(0, 2, 2, &[7; PAGE_SIZE]).is_err());
    assert_eq!(page.page(), original);
    assert!(page.delete_record(1).is_err());
    assert_eq!(page.page(), original);
    page.compact().unwrap();
    assert_eq!(&page.page()[PAGE_SIZE..], &original[PAGE_SIZE..]);
}
