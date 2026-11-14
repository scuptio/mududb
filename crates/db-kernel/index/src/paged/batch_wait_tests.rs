//! Deterministic cancellation/wakeup checks and mixed inline/overflow scan parity.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    tests::{context, key},
    *,
};
use crate::index_key::key_tuple::KeyTuple;
use futures::poll;
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
async fn cancelled_leaf_wait_does_not_publish_or_leak_a_latch() {
    let tree = tree();
    tree.insert(key(10), 10).await.unwrap();
    let access = tree.access().await;
    let page = tree.pin(&access, &tree.root_link()).await.unwrap();
    let reader = page.read().await;
    let before = tree.performance_stats();
    {
        let mut cancelled = Box::pin(tree.insert(key(10), 20));
        assert!(poll!(&mut cancelled).is_pending());
        assert_eq!(tree.performance_stats().latch_waits, before.latch_waits + 1);
    }
    // Cancellation removes the queued writer; a new reader must complete now.
    assert_eq!(tree.get(&key(10)).await.unwrap(), Some(10));
    let mut update = Box::pin(tree.insert(key(10), 30));
    assert!(poll!(&mut update).is_pending());
    drop(reader);
    assert_eq!(update.await.unwrap(), Some(10));
    assert_eq!(tree.get(&key(10)).await.unwrap(), Some(30));
    let after = tree.performance_stats();
    assert_eq!(after.cow_bytes, before.cow_bytes);
    assert_eq!(after.retries, before.retries);
    assert_eq!(after.latch_waits, before.latch_waits + 2);
}

#[tokio::test]
async fn cached_reads_observe_publications_in_order() {
    let tree = tree();
    tree.insert(key(10), 10).await.unwrap();
    let query = key(10);
    // The first visit on a page version only marks it seen; the second
    // decodes and caches. Two warm-up reads populate the decoded-node cache
    // before the leaf is latched.
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    let access = tree.access().await;
    let page = tree.pin(&access, &tree.root_link()).await.unwrap();
    let guard = page.write().await;
    // A cached read completes without the leaf latch and legitimately
    // observes the pre-publish version: the write has not been published yet.
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    let edit = patch::replace(&view::NodeView::new(&guard).unwrap(), 0, u128::MAX).unwrap();
    let mut prepared = guard
        .prepare_edit(edit, Some(&images::VALIDATOR), Some(&images::WRITEBACK))
        .unwrap();
    prepared.publish();
    drop(prepared);
    // The publication bumped the write epoch, invalidating the cached node:
    // the next read sees the published value.
    assert_eq!(tree.get(&query).await.unwrap(), Some(u128::MAX));
    assert_eq!(tree.performance_stats().retries, 0);
    assert_eq!(tree.performance_stats().latch_waits, 0);
}

#[tokio::test]
async fn queued_writer_acquires_after_held_read_regardless_of_cached_reads() {
    let tree = tree();
    tree.insert(key(10), 10).await.unwrap();
    let query = key(10);
    // Two warm-up reads populate the decoded-node cache (first visit marks,
    // second decodes): subsequent reads never touch the page latch.
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    let access = tree.access().await;
    let page = tree.pin(&access, &tree.root_link()).await.unwrap();
    let held = page.read().await;
    let mut writer = Box::pin(page.write());
    assert!(poll!(&mut writer).is_pending());
    // Cached reads complete without the leaf latch: they neither barge ahead
    // of nor delay the queued writer, which only waits for the current holder.
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    drop(held);
    let _write = writer.await;
    assert_eq!(tree.get(&query).await.unwrap(), Some(10));
    assert_eq!(tree.performance_stats().latch_probe_hits, 0);
}

#[tokio::test]
async fn batched_scan_preserves_mixed_overflow_prefixes_and_bounds() {
    let tree = PagedBTree::with_row_ids(
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
    .with_lexicographic_keys();
    let mut expected = Vec::new();
    for i in 0..40u8 {
        let mut bytes = b"shared-prefix-".to_vec();
        bytes.push(i);
        bytes.resize(if i % 3 == 0 { 700 } else { 24 }, i);
        tree.insert(KeyTuple::from(bytes.clone()), u128::from(i))
            .await
            .unwrap();
        expected.push((bytes, u128::from(i)));
    }
    let low = KeyTuple::from(expected[4].0.clone());
    let high = KeyTuple::from(expected[35].0.clone());
    for (bounds, expected) in [
        ((Bound::Unbounded, Bound::Unbounded), &expected[..]),
        (
            (Bound::Excluded(&low), Bound::Included(&high)),
            &expected[5..36],
        ),
    ] {
        let rows = tree.range(bounds).await.unwrap();
        let actual: Vec<_> = rows.into_iter().map(|(k, v)| (k.into_inner(), v)).collect();
        assert_eq!(actual, expected);
    }
}

#[tokio::test]
async fn direct_leaf_writer_rechecks_a_fence_changed_while_waiting() {
    use super::node::Page;
    let tree = tree();
    for i in 0..900 {
        tree.insert(key(i), u128::from(i)).await.unwrap();
    }
    let access = tree.access().await;
    let query = key(899);
    let (page, image, _, _) = tree.view_leaf(&access, query.as_slice()).await.unwrap();
    let node = tree
        .decode_image(&image, page.id())
        .await
        .unwrap()
        .node()
        .unwrap()
        .clone();
    drop(image);
    let (mut left, _, right) = split_point::divide(&node, tree.buffer.page_size()).unwrap();
    let right = tree.allocate(&access, Page::Node(right)).await.unwrap();
    left.right = right;
    left.revision += 1;
    // Emulate a split between parent traversal and exclusive leaf acquisition.
    // Allocate/encode before latching; only infallible publication occurs latched.
    let replacement = tree.image(page.id(), left).unwrap();
    let guard = page.write().await;
    let mut update = Box::pin(tree.insert(query.clone(), u128::MAX));
    assert!(poll!(&mut update).is_pending());
    let mut prepared = guard.prepare(replacement).unwrap();
    prepared.publish();
    drop(prepared);
    assert_eq!(update.await.unwrap(), Some(899));
    assert_eq!(tree.get(&query).await.unwrap(), Some(u128::MAX));
    assert_eq!(tree.get(&key(898)).await.unwrap(), Some(898));
    // The exact count drains in-flight operations through an exclusive buffer
    // lease, which cannot coexist with this test's retained pin/lease.
    drop(page);
    drop(access);
    // The stale parent separator is legal while a B-link split is propagated.
    assert_eq!(tree.len().await.unwrap(), 900);
}
