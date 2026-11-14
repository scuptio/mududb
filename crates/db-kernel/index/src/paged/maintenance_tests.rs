#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    node::{Link, Page},
    tests::{context, key},
    *,
};
use futures::{poll, FutureExt};
use std::rc::Rc;
use std::{ops::Bound, sync::Arc};

fn options() -> BufferOptions {
    BufferOptions {
        page_size: 512,
        frames: 32,
        partitions: 1,
        ..BufferOptions::default()
    }
}
fn tree() -> PagedBTree<u128> {
    PagedBTree::with_row_ids(context(), options(), Arc::new(MemoryPageStore::default())).unwrap()
}

#[tokio::test]
async fn merges_internal_nodes_shrinks_root_and_reuses_overflow_pages() {
    let tree = tree();
    for i in 0..500 {
        tree.insert(key(i), i as u128).await.unwrap();
    }
    assert!(
        tree.snapshot(&tree.buffer.access().await, &tree.root_link())
            .await
            .unwrap()
            .1
            .level
            >= 2
    );
    for i in 1..500 {
        tree.remove(&key(i)).await.unwrap();
    }
    let high = tree
        .buffer
        .allocator_state(&tree.buffer.access().await)
        .unwrap()
        .next_page;
    let mut merged = 0;
    for _ in 0..10 {
        merged += tree.maintain().await.unwrap().merged;
    }
    assert!(merged > 0);
    assert!(tree
        .snapshot(&tree.buffer.access().await, &tree.root_link())
        .await
        .unwrap()
        .1
        .leaf());
    assert_eq!(tree.get(&key(0)).await.unwrap(), Some(0));
    let long = crate::index_key::key_tuple::KeyTuple::from(vec![255; 8000]);
    tree.insert(long.clone(), u128::MAX).await.unwrap();
    assert_eq!(
        tree.buffer
            .allocator_state(&tree.buffer.access().await)
            .unwrap()
            .next_page,
        high
    );
    assert_eq!(tree.get(&long).await.unwrap(), Some(u128::MAX));
    tree.remove(&long).await.unwrap();
    assert!(tree.reclaim_pages().await.unwrap() > 10);
    tree.insert(long.clone(), 7).await.unwrap();
    assert_eq!(tree.get(&long).await.unwrap(), Some(7));
    assert_eq!(
        tree.buffer
            .allocator_state(&tree.buffer.access().await)
            .unwrap()
            .next_page,
        high
    );
}

#[tokio::test]
async fn stale_paths_forward_and_reuse_waits_for_readers() {
    let tree = tree();
    for i in 0..100 {
        tree.insert(key(i), i as u128).await.unwrap();
    }
    for i in 1..100 {
        tree.remove(&key(i)).await.unwrap();
    }
    // A held buffer epoch is the grace period that keeps the stale path's
    // pages from being reused while it is inspected.
    let epoch = tree.buffer.access().await;
    let path = tree
        .path(&tree.buffer.access().await, key(99).as_slice())
        .await
        .unwrap();
    let stale = path.last().unwrap().0.clone();
    let access = tree.buffer.access().await;
    let (pin, before) = tree.snapshot(&access, &stale).await.unwrap();
    assert!(before.leaf());
    assert!(tree.merge_pages().await.unwrap() > 0);
    let after = tree
        .decode_image(&pin.snapshot().await, pin.id())
        .await
        .unwrap()
        .node()
        .unwrap()
        .clone();
    assert!(!after.leaf());
    assert_ne!(before.revision, after.revision);
    let mut gc = tree.reclaim_pages().boxed_local();
    assert!(poll!(&mut gc).is_pending());
    // Cancel waiting GC; the still-live epoch can continue using the forwarding page.
    drop(gc);
    let (_, forwarded) = tree
        .snapshot(&tree.buffer.access().await, &after.left)
        .await
        .unwrap();
    assert!(forwarded.leaf());
    drop(pin);
    drop(access);
    drop(epoch);
    assert!(tree.reclaim_pages().await.unwrap() > 0);
    assert_eq!(tree.get(&key(0)).await.unwrap(), Some(0));
}

#[tokio::test(flavor = "current_thread")]
async fn merging_overlaps_readers_writers_and_preserves_sorted_mapping() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let tree = Rc::new(tree());
            for i in 0..300 {
                tree.insert(key(i), i as u128).await.unwrap();
            }
            for i in 0..300 {
                if i % 3 != 0 {
                    tree.remove(&key(i)).await.unwrap();
                }
            }
            let mut tasks = Vec::new();
            for worker in 0..3 {
                let tree = tree.clone();
                tasks.push(tokio::task::spawn_local(async move {
                    for round in 0..20 {
                        for i in (worker * 3..300).step_by(9) {
                            assert!(tree.get(&key(i)).await.unwrap().is_some());
                            tree.insert(key(i), (round * 1000 + i) as u128)
                                .await
                                .unwrap();
                        }
                    }
                }));
            }
            let maintenance = {
                let tree = tree.clone();
                tokio::task::spawn_local(async move {
                    for _ in 0..15 {
                        tree.maintain().await.unwrap();
                    }
                })
            };
            for task in tasks {
                task.await.unwrap();
            }
            maintenance.await.unwrap();
            let all = tree
                .range((Bound::Unbounded, Bound::Unbounded))
                .await
                .unwrap();
            assert_eq!(all.len(), 100);
            for (at, (k, v)) in all.iter().enumerate() {
                assert_eq!(k.as_slice(), key(at as u64 * 3).as_slice());
                assert_eq!(*v, 19000 + at as u128 * 3);
            }
        })
        .await;
}

#[tokio::test]
async fn online_tree_checkpoint_can_restore_after_reuse_and_remains_open() {
    let store = Arc::new(MemoryPageStore::default());
    let tree = PagedBTree::with_row_ids(context(), options(), store.clone()).unwrap();
    for i in 0..100 {
        tree.insert(key(i), u128::MAX - i as u128).await.unwrap();
    }
    tree.clear().await.unwrap();
    assert!(tree.reclaim_pages().await.unwrap() > 1);
    for i in 0..100 {
        tree.insert(key(i), i as u128).await.unwrap();
    }
    let state = tree.checkpoint().await.unwrap();
    let restored = PagedBTree::restore_row_ids(context(), options(), store, &state)
        .await
        .unwrap();
    for i in 0..100 {
        assert_eq!(restored.get(&key(i)).await.unwrap(), Some(i as u128));
    }
    drop(restored);
    tree.insert(key(100), 100).await.unwrap();
    assert_eq!(tree.len().await.unwrap(), 101);
}

#[tokio::test]
async fn orphan_allocations_are_reclaimed_without_touching_root() {
    let tree = tree();
    let orphan = tree
        .allocate(
            &tree.buffer.access().await,
            Page::Overflow {
                next: 0,
                bytes: vec![11],
            },
        )
        .await
        .unwrap();
    let access = tree.buffer.access().await;
    let old = tree.pin(&access, &orphan).await.unwrap();
    drop(old);
    drop(access);
    tree.reclaim_pages().await.unwrap();
    let reused = tree
        .allocate(
            &tree.buffer.access().await,
            Page::Overflow {
                next: 0,
                bytes: vec![22],
            },
        )
        .await
        .unwrap();
    assert_eq!(orphan.pid, reused.pid);
    // A retained advisory hint must not return the discarded frame contents.
    let access = tree.buffer.access().await;
    let pin = tree.pin(&access, &Link::new(reused.pid)).await.unwrap();
    match &tree
        .decode_image(&pin.snapshot().await, pin.id())
        .await
        .unwrap()
    {
        Page::Overflow { bytes, .. } => assert_eq!(bytes, &vec![22]),
        _ => panic!("expected reused overflow image"),
    };
}

#[tokio::test(flavor = "current_thread")]
async fn concurrent_splits_merges_and_reuse_preserve_disjoint_writers() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let tree = Rc::new(tree());
            let mut tasks = Vec::new();
            for worker in 0..3 {
                let tree = tree.clone();
                tasks.push(tokio::task::spawn_local(async move {
                    for round in 0..6 {
                        for i in worker * 100..(worker + 1) * 100 {
                            assert_eq!(tree.insert(key(i), round).await.unwrap(), None);
                        }
                        if round != 5 {
                            for i in worker * 100..(worker + 1) * 100 {
                                assert_eq!(tree.remove(&key(i)).await.unwrap(), Some(round));
                            }
                        }
                    }
                }));
            }
            let maintenance = {
                let tree = tree.clone();
                tokio::task::spawn_local(async move {
                    for _ in 0..40 {
                        tree.maintain().await.unwrap();
                    }
                })
            };
            for task in tasks {
                task.await.unwrap();
            }
            maintenance.await.unwrap();
            let all = tree
                .range((Bound::Unbounded, Bound::Unbounded))
                .await
                .unwrap();
            assert_eq!(all.len(), 300);
            for (at, (k, v)) in all.iter().enumerate() {
                assert_eq!(k.as_slice(), key(at as u64).as_slice());
                assert_eq!(*v, 5);
            }
        })
        .await;
}

#[tokio::test]
async fn reclamation_keeps_shared_overflow_separators_and_fences_alive() {
    let tree = tree();
    let keys: Vec<_> = (0..60u64)
        .map(|i| {
            let mut bytes = i.to_be_bytes().to_vec();
            bytes.extend_from_slice(&vec![i as u8; 1000]);
            crate::index_key::key_tuple::KeyTuple::from(bytes)
        })
        .collect();
    for (i, k) in keys.iter().enumerate() {
        tree.insert(k.clone(), i as u128).await.unwrap();
    }
    for (i, k) in keys.iter().enumerate() {
        if i % 3 != 0 {
            tree.remove(k).await.unwrap();
        }
    }
    for _ in 0..4 {
        tree.maintain().await.unwrap();
    }
    let all = tree
        .range((Bound::Unbounded, Bound::Unbounded))
        .await
        .unwrap();
    assert_eq!(all.len(), 20);
    for (i, k) in keys.iter().enumerate() {
        assert_eq!(
            tree.get(k).await.unwrap(),
            (i % 3 == 0).then_some(i as u128)
        );
    }
    for (i, k) in keys.iter().enumerate() {
        tree.insert(k.clone(), i as u128).await.unwrap();
    }
    tree.maintain().await.unwrap();
    assert_eq!(tree.len().await.unwrap(), 60);
}
