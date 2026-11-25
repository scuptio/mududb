#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{
    btree::btree_index::BTreeIndex,
    index_key::{compare_context::CompareContext, key_tuple::KeyTuple},
};
use mudu_contract::tuple::{comparator::TupleComparator, tuple_binary_desc::TupleBinaryDesc};
use mudu_type::{data_type::DataType, type_family::TypeFamily};
use std::rc::Rc;
use std::{ops::Bound, sync::Arc};

pub(super) fn context() -> CompareContext {
    CompareContext {
        result: Ok(()),
        comparator: TupleComparator {
            compare: |a, b, _| Ok(a.cmp(b)),
            equal: |a, b, _| Ok(a == b),
            hash_cal_one: |a, _, h| {
                h.write(a);
                Ok(())
            },
            hash_cal_finish: |a, _, h| {
                h.write(a);
                Ok(h.finish())
            },
        },
        desc: TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::I32)]).unwrap(),
    }
}
fn small() -> PagedBTree<u64> {
    PagedBTree::with_store(
        context(),
        BufferOptions {
            page_size: 512,
            frames: 16,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap()
}
pub(super) fn key(i: u64) -> KeyTuple {
    KeyTuple::from(i.to_be_bytes().to_vec())
}
fn raw(items: Vec<(KeyTuple, u64)>) -> Vec<(Vec<u8>, u64)> {
    items
        .into_iter()
        .map(|(k, v)| (k.into_inner(), v))
        .collect()
}

#[tokio::test]
async fn random_parity_splits_evictions_and_deletes() {
    let tree = small();
    let old = BTreeIndex::new(context());
    let mut seed = 321u64;
    for step in 0..3000 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let k = key(seed % 700);
        if seed & 4 == 0 {
            assert_eq!(tree.remove(&k).await.unwrap(), old.remove(&k).unwrap());
        } else {
            assert_eq!(
                tree.insert(k.clone(), step).await.unwrap(),
                old.insert(k.clone(), step).unwrap()
            );
        }
        assert_eq!(tree.get(&k).await.unwrap(), old.get(&k).unwrap());
        if step % 100 == 0 {
            assert_eq!(tree.len().await.unwrap(), old.len().unwrap());
            assert_eq!(
                raw(tree
                    .range((Bound::Unbounded, Bound::Unbounded))
                    .await
                    .unwrap()),
                raw(old.range((Bound::Unbounded, Bound::Unbounded)).unwrap())
            );
        }
    }
    assert!(tree.buffer_stats().evictions > 0);
    while !old.is_empty().unwrap() {
        let actual = tree
            .pop_first()
            .await
            .unwrap()
            .map(|(k, v)| (k.into_inner(), v));
        let expected = old.pop_first().unwrap().map(|(k, v)| (k.into_inner(), v));
        assert_eq!(actual, expected);
    }
    assert_eq!(tree.len().await.unwrap(), 0);
    tree.insert(key(12), 12).await.unwrap();
    tree.clear().await.unwrap();
    assert!(tree.is_empty().await.unwrap());
}

#[tokio::test]
async fn overflow_and_range_bounds() {
    let tree = small();
    for i in 0..80u8 {
        let mut bytes = vec![i; 2000];
        bytes[0] = 1;
        tree.insert(KeyTuple::from(bytes), u64::from(i))
            .await
            .unwrap();
    }
    let mut low = vec![20; 2000];
    low[0] = 1;
    let mut high = vec![40; 2000];
    high[0] = 1;
    let (low, high) = (KeyTuple::from(low), KeyTuple::from(high));
    let range = tree
        .range((Bound::Excluded(&low), Bound::Included(&high)))
        .await
        .unwrap();
    assert_eq!(range.len(), 20);
    assert_eq!(range[0].1, 21);
    assert_eq!(range[19].1, 40);
    assert_eq!(tree.pop_last().await.unwrap().unwrap().1, 79);
    tree.flush_pages().await.unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_writers_readers_and_scans() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let tree = Rc::new(
                PagedBTree::with_store(
                    context(),
                    BufferOptions {
                        page_size: 512,
                        frames: 128,
                        partitions: 2,
                        ..BufferOptions::default()
                    },
                    Arc::new(MemoryPageStore::default()),
                )
                .unwrap(),
            );
            let mut tasks = Vec::new();
            for worker in 0..4 {
                let tree = tree.clone();
                tasks.push(tokio::task::spawn_local(async move {
                    for i in 0..300 {
                        let n = worker * 1000 + i;
                        tree.insert(key(n), n).await.unwrap();
                        assert_eq!(tree.get(&key(n)).await.unwrap(), Some(n));
                        if i % 7 == 0 {
                            tree.remove(&key(n)).await.unwrap();
                        }
                    }
                }));
            }
            let scanner = tree.clone();
            tasks.push(tokio::task::spawn_local(async move {
                for _ in 0..20 {
                    let items = raw(scanner
                        .range((Bound::Unbounded, Bound::Unbounded))
                        .await
                        .unwrap());
                    assert!(items.windows(2).all(|p| p[0].0 < p[1].0));
                    tokio::task::yield_now().await;
                }
            }));
            for task in tasks {
                task.await.unwrap();
            }
            assert_eq!(tree.len().await.unwrap(), 4 * (300 - 43));
        })
        .await;
}

#[tokio::test]
async fn compare_error_leaves_mapping_unchanged() {
    fn fail(a: &[u8], b: &[u8], _: &TupleBinaryDesc) -> RS<std::cmp::Ordering> {
        if a == [9] || b == [9] {
            return Err(mudu_error!(ErrorCode::ComparisonFailed, "injected"));
        }
        Ok(a.cmp(b))
    }
    let mut ctx = context();
    ctx.comparator.compare = fail;
    let tree = PagedBTree::new(ctx).unwrap();
    tree.insert(KeyTuple::from(vec![1]), 1).await.unwrap();
    assert!(tree.insert(KeyTuple::from(vec![9]), 2).await.is_err());
    assert!(tree.remove(&KeyTuple::from(vec![9])).await.is_err());
    assert_eq!(tree.len().await.unwrap(), 1);
}

#[tokio::test]
async fn cancellation_releases_scan_lease_and_pins() {
    use futures::{poll, FutureExt};
    let tree = small();
    // An exclusive maintenance lease blocks later operations the way the
    // retired tree gate did; cancelling a waiter must not poison either.
    let held = tree.buffer.maintenance().await;
    let mut insert = Box::pin(tree.insert(key(1), 1));
    assert!(poll!(&mut insert).is_pending());
    drop(insert);
    drop(held);
    tree.insert(key(2), 2).await.unwrap();
    let access = tree.buffer.access().await;
    let root = tree
        .pin(&access, &node::Link::new(tree.root.get()))
        .await
        .unwrap();
    let held = root.write().await;
    let lookup = key(2);
    let mut read = tree.get(&lookup).boxed_local();
    assert!(poll!(&mut read).is_pending());
    drop(read);
    drop(held);
    drop(root);
    assert_eq!(tree.get(&key(2)).await.unwrap(), Some(2));
}

#[test]
fn codec_rejects_corruption() {
    for size in [512, 4096, 16384] {
        let mut node = node::Node::empty(1);
        node.left = node::Link::new(9);
        node.high = Some(node::Key::inline(b"different-high-fence"));
        node.right = node::Link::new(19);
        for n in 0..6 {
            Arc::make_mut(&mut node.entries).push(node::Entry {
                key: node::Key::inline(format!("shared-prefix-{n}").as_bytes()),
                target: node::Link::new(n + 10),
                upper: u64::MAX - n,
            });
        }
        for page in [
            node::Page::Node(node),
            node::Page::Overflow {
                next: 25,
                bytes: vec![42; size - 64],
            },
        ] {
            let mut bytes = codec::encode(&page, size, 7).unwrap();
            let decoded = codec::decode(&bytes, size, 7).unwrap();
            assert!(codec::decode(&bytes, size, 8).is_err());
            assert_eq!(codec::encode(&decoded, size, 7).unwrap(), bytes);
            bytes[size - 1] ^= 1;
            assert!(codec::decode(&bytes, size, 7).is_err());
            assert!(codec::decode(&bytes[..size - 1], size, 7).is_err());
        }
    }
}

#[tokio::test]
async fn inline_row_ids_survive_checkpoint_without_value_directory() {
    let store = Arc::new(MemoryPageStore::default());
    let options = BufferOptions {
        page_size: 512,
        frames: 16,
        partitions: 1,
        ..BufferOptions::default()
    };
    let tree = PagedBTree::with_row_ids(context(), options, store.clone()).unwrap();
    for i in 0..500u64 {
        tree.insert(key(i), (u128::from(i) << 80) | u128::from(i))
            .await
            .unwrap();
    }
    assert_eq!(
        tree.insert(key(1), u128::MAX).await.unwrap(),
        Some((1u128 << 80) | 1)
    );
    for shard in &tree.values {
        assert!(shard.lock().await.is_empty());
    }
    let checkpoint = tree.close_checkpoint().await.unwrap();
    assert!(tree.insert(key(600), 6).await.is_err());
    let reopened = PagedBTree::restore_row_ids(context(), options, store, &checkpoint)
        .await
        .unwrap();
    assert_eq!(reopened.get(&key(1)).await.unwrap(), Some(u128::MAX));
    assert_eq!(
        reopened.get(&key(400)).await.unwrap(),
        Some((400u128 << 80) | 400)
    );
    assert_eq!(reopened.remove(&key(1)).await.unwrap(), Some(u128::MAX));
    assert_eq!(reopened.len().await.unwrap(), 499);
}

#[tokio::test(flavor = "current_thread")]
async fn competing_replacements_have_a_serial_history() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let tree = Rc::new(small());
            for iteration in 0..100 {
                tree.insert(key(iteration), 0).await.unwrap();
                let a = tree.clone();
                let b = tree.clone();
                let a = tokio::task::spawn_local(async move {
                    a.insert(key(iteration), 1).await.unwrap().unwrap()
                });
                let b = tokio::task::spawn_local(async move {
                    b.insert(key(iteration), 2).await.unwrap().unwrap()
                });
                let (a, b) = (a.await.unwrap(), b.await.unwrap());
                let final_value = tree.get(&key(iteration)).await.unwrap().unwrap();
                assert!(
                    (a == 0 && b == 1 && final_value == 2)
                        || (b == 0 && a == 2 && final_value == 1)
                );
            }
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn ascending_inserts_append_packed_right_edge_pages() {
    // At 4 KiB pages the fence key added to the left page is small relative
    // to page capacity, so right-edge appends pack leaves completely.
    let tree = PagedBTree::with_store(
        context(),
        BufferOptions {
            page_size: 4096,
            frames: 64,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap();
    // Ascending keys pack the rightmost leaf completely instead of
    // splitting it in half on every fill.
    for i in 0..2000u64 {
        assert_eq!(tree.insert(key(i * 2), i).await.unwrap(), None);
    }
    let access = tree.access().await;
    let allocated = tree.buffer.allocator_state(&access).unwrap().next_page;
    drop(access);
    assert!(
        allocated < 35,
        "right-edge appends pack leaves tighter than half-full splits (plain divide: 39 pages), got next_page={allocated}"
    );
    // Interleaved inserts into packed leaves still take the divide path.
    for i in 0..2000u64 {
        assert_eq!(tree.insert(key(i * 2 + 1), i).await.unwrap(), None);
    }
    for i in 0..2000u64 {
        assert_eq!(tree.get(&key(i * 2)).await.unwrap(), Some(i));
        assert_eq!(tree.get(&key(i * 2 + 1)).await.unwrap(), Some(i));
    }
    let all = tree
        .range((Bound::Unbounded, Bound::Unbounded))
        .await
        .unwrap();
    assert_eq!(all.len(), 4000);
    for (i, (k, v)) in all.iter().enumerate() {
        assert_eq!(k.as_slice(), key(i as u64).as_slice());
        assert_eq!(*v, (i as u64) / 2);
    }
}
