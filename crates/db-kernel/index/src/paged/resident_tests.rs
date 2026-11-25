//! Regression checks for synchronous completion, exact scan bounds, and diagnostics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    tests::{context, key},
    *,
};
use crate::index_key::key_tuple::KeyTuple;
use std::{ops::Bound, sync::Arc};

fn tree() -> PagedBTree<u128> {
    PagedBTree::with_row_ids(
        context(),
        BufferOptions::default(),
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap()
    .with_diagnostics()
}
#[tokio::test]
async fn diagnostics_observe_cow_moves_and_no_copy_for_unique_updates() {
    let tree = tree();
    for id in [10, 20, 30] {
        tree.insert(key(id), id as u128).await.unwrap();
    }
    let before = tree.performance_stats();
    tree.insert(key(20), u128::MAX).await.unwrap();
    assert_eq!(tree.performance_stats().cow_bytes, before.cow_bytes);
    let access = tree.access().await;
    let (_, retained) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    tree.insert(key(20), 0).await.unwrap();
    assert_eq!(
        tree.performance_stats().cow_bytes - before.cow_bytes,
        tree.buffer.page_size() as u64
    );
    assert_eq!(
        view::NodeView::new(&retained).unwrap().entry(1).unwrap().1,
        u128::MAX
    );
    drop(retained);
    let before = tree.performance_stats();
    tree.insert(key(15), 15).await.unwrap();
    assert!(tree.performance_stats().moved_bytes > before.moved_bytes);
    assert_eq!(tree.performance_stats().cow_bytes, before.cow_bytes);
}
#[tokio::test]
async fn scan_slots_preserve_inclusive_exclusive_and_empty_ranges() {
    let tree = tree();
    for id in 0..900 {
        tree.insert(key(id * 2), id as u128).await.unwrap();
    }
    for (low, high) in [
        (0, 0),
        (21, 23),
        (100, 100),
        (100, 180),
        (1798, 2000),
        (2000, 3000),
    ] {
        let (low_key, high_key) = (key(low), key(high));
        for include_low in [false, true] {
            for include_high in [false, true] {
                if low == high && !include_low && !include_high {
                    continue;
                }
                let bounds = (
                    if include_low {
                        Bound::Included(&low_key)
                    } else {
                        Bound::Excluded(&low_key)
                    },
                    if include_high {
                        Bound::Included(&high_key)
                    } else {
                        Bound::Excluded(&high_key)
                    },
                );
                let rows = tree.range(bounds).await.unwrap();
                let expected: Vec<u128> = (0..900)
                    .filter(|id| {
                        let k = id * 2;
                        (k > low || (include_low && k == low))
                            && (k < high || (include_high && k == high))
                    })
                    .map(|id| id as u128)
                    .collect();
                assert_eq!(
                    rows.into_iter().map(|(_, v)| v).collect::<Vec<_>>(),
                    expected
                );
            }
        }
    }
}
#[tokio::test]
async fn page_sized_inline_threshold_avoids_overflow_and_survives_restore() {
    let store = Arc::new(MemoryPageStore::default());
    let options = BufferOptions::default();
    let tree = PagedBTree::with_row_ids(context(), options, store.clone())
        .unwrap()
        .with_diagnostics();
    for id in 0..64u8 {
        let mut bytes = vec![7; 2048];
        bytes[0] = id;
        tree.insert(KeyTuple::from(bytes), id as u128)
            .await
            .unwrap();
    }
    let state = tree.checkpoint().await.unwrap();
    let restored = PagedBTree::restore_row_ids(context(), options, store, &state)
        .await
        .unwrap()
        .with_diagnostics();
    for id in 0..64u8 {
        let mut bytes = vec![7; 2048];
        bytes[0] = id;
        assert_eq!(
            restored.get(&KeyTuple::from(bytes)).await.unwrap(),
            Some(id as u128)
        );
    }
    assert_eq!(restored.performance_stats().overflow_reads, 0);
}
#[tokio::test]
async fn old_512_byte_head_overflow_pages_remain_readable() {
    use super::node::{Entry, Key, Link, Node, Page};
    let tree = tree();
    let bytes = vec![11; 2048];
    let access = tree.access().await;
    let overflow = tree
        .allocate(
            &access,
            Page::Overflow {
                next: 0,
                bytes: bytes[512..].to_vec(),
            },
        )
        .await
        .unwrap();
    let mut node = Node::empty(0);
    Arc::make_mut(&mut node.entries).push(Entry {
        key: Key {
            head: bytes[..512].to_vec(),
            overflow: overflow.pid,
            len: 2048,
        },
        target: Link::new(42),
        upper: 0,
    });
    let (page, _) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    page.write()
        .await
        .prepare(tree.image(page.id(), node).unwrap())
        .unwrap()
        .publish();
    tree.count.set(1);
    drop(page);
    drop(access);
    assert_eq!(
        tree.get(&KeyTuple::from(bytes.clone())).await.unwrap(),
        Some(42)
    );
    assert!(tree.performance_stats().overflow_reads > 0);
    assert_eq!(tree.remove(&KeyTuple::from(bytes)).await.unwrap(), Some(42));
}

#[test]
fn head_budget_respects_comparator_capability() {
    let tree = tree();
    assert_eq!(tree.inline_key_len(2048), 2048);
    assert_eq!(tree.with_lexicographic_keys().inline_key_len(2048), 512);
}

#[tokio::test]
async fn write_memo_appends_then_validates_envelope_and_survives_clear() {
    let tree = tree();
    for i in 0..900 {
        tree.insert(key(i), u128::from(i)).await.unwrap();
    }
    // The append stream leaves the memo on the rightmost leaf.
    assert_ne!(tree.memo.get(), 0);
    // A key below every memoized-leaf entry cannot prove membership without
    // a descent: it must fall back and still land in its rightful leaf.
    tree.insert(key(0), u128::MAX).await.unwrap();
    assert_eq!(tree.get(&key(0)).await.unwrap(), Some(u128::MAX));
    assert_eq!(tree.get(&key(899)).await.unwrap(), Some(899));
    // A split moves the append target; the stale memo fails its fence check
    // once, then re-finds the stream's leaf.
    for i in 900..1800 {
        tree.insert(key(i), u128::from(i)).await.unwrap();
    }
    for i in 0..1800 {
        let expected = if i == 0 { u128::MAX } else { i as u128 };
        assert_eq!(tree.get(&key(i)).await.unwrap(), Some(expected));
    }
    // clear() leaves orphaned leaf pages with intact bytes; the memo must not
    // validate against them, or the next insert would publish into an
    // unreachable page and lose the mapping.
    tree.clear().await.unwrap();
    assert_eq!(tree.memo.get(), 0);
    tree.insert(key(5), 55).await.unwrap();
    assert_eq!(tree.get(&key(5)).await.unwrap(), Some(55));
    assert_eq!(tree.len().await.unwrap(), 1);
}

#[tokio::test]
async fn write_memo_deletes_in_order_and_reports_absent_keys() {
    let tree = tree();
    for i in 0..600 {
        tree.insert(key(i), u128::from(i)).await.unwrap();
    }
    // Ascending deletes ride the memo; deleting an absent smaller key must
    // not be confused with a hit just because the memo leaf accepted it.
    for i in 0..300 {
        assert_eq!(tree.remove(&key(i)).await.unwrap(), Some(i as u128));
    }
    assert_eq!(tree.remove(&key(0)).await.unwrap(), None);
    for i in 300..600 {
        assert_eq!(tree.get(&key(i)).await.unwrap(), Some(i as u128));
    }
    assert_eq!(tree.len().await.unwrap(), 300);
}

#[tokio::test]
async fn batch_insert_and_get_preserve_order_and_return_old_values() {
    let tree = tree();
    let pairs: Vec<(KeyTuple, u128)> = (0..500).map(|i| (key(i), u128::from(i))).collect();
    let olds = tree.insert_batch(&pairs).await.unwrap();
    assert!(olds.iter().all(|old| old.is_none()));
    let reversed: Vec<KeyTuple> = (0..500).rev().map(key).collect();
    let found = tree.get_batch(&reversed).await.unwrap();
    for (i, id) in found.iter().enumerate() {
        assert_eq!(*id, Some((499 - i) as u128));
    }
    let doubled: Vec<(KeyTuple, u128)> = (0..500).map(|i| (key(i), u128::from(i) * 2)).collect();
    let olds = tree.insert_batch(&doubled).await.unwrap();
    for (i, old) in olds.iter().enumerate() {
        assert_eq!(*old, Some(i as u128));
    }
    assert_eq!(tree.len().await.unwrap(), 500);
    // Missing keys report None in place; duplicates inside one batch chain.
    let mixed: Vec<KeyTuple> = vec![key(0), key(1000), key(499), key(1001)];
    assert_eq!(
        tree.get_batch(&mixed).await.unwrap(),
        vec![Some(0), None, Some(499 * 2), None]
    );
    let dup: Vec<(KeyTuple, u128)> = vec![(key(2000), 1), (key(2000), 2)];
    let olds = tree.insert_batch(&dup).await.unwrap();
    assert_eq!(olds, vec![None, Some(1)]);
    assert_eq!(tree.get(&key(2000)).await.unwrap(), Some(2));
}
