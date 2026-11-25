//! Integration checks for the new view/patch paths and their concurrency contracts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    tests::{context, key},
    *,
};
use crate::index_key::key_tuple::KeyTuple;
use futures::{poll, FutureExt};
use std::{ops::Bound, sync::Arc};

fn inline() -> PagedBTree<u128> {
    PagedBTree::with_row_ids(
        context(),
        BufferOptions {
            page_size: 512,
            frames: 32,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap()
}

#[tokio::test]
async fn inline_operations_never_wait_on_the_generic_value_directory() {
    let tree = inline();
    let mut held = Vec::new();
    for shard in &tree.values {
        held.push(shard.lock().await);
    }
    tree.next_value.set(u64::MAX);
    // The future can yield for split coordination, but must never wait for held shards.
    let run = async {
        for i in 0..100 {
            tree.insert(key(i), i as u128).await.unwrap();
        }
        for i in 0..100 {
            assert_eq!(tree.get(&key(i)).await.unwrap(), Some(i as u128));
            assert_eq!(
                tree.insert(key(i), u128::MAX).await.unwrap(),
                Some(i as u128)
            );
            assert_eq!(tree.remove(&key(i)).await.unwrap(), Some(u128::MAX));
        }
        tree.clear().await.unwrap();
    };
    mudu_sys::timeout(std::time::Duration::from_secs(5), run)
        .await
        .unwrap();
    assert_eq!(tree.next_value.get(), u64::MAX);
}

#[tokio::test]
async fn replacement_does_not_allocate_a_generic_value_id() {
    let tree = PagedBTree::new(context()).unwrap();
    tree.insert(key(1), 1).await.unwrap();
    tree.next_value.set(u64::MAX);
    assert_eq!(tree.insert(key(1), 2).await.unwrap(), Some(1));
    assert_eq!(tree.get(&key(1)).await.unwrap(), Some(2));
    assert!(tree.insert(key(2), 3).await.is_err());
    assert_eq!(tree.len().await.unwrap(), 1);
}

#[tokio::test]
async fn comparator_equivalence_returns_the_original_stored_key() {
    let mut ctx = context();
    ctx.comparator.compare = |a, b, _| {
        Ok(a.iter()
            .map(u8::to_ascii_lowercase)
            .cmp(b.iter().map(u8::to_ascii_lowercase)))
    };
    let tree = PagedBTree::with_row_ids(
        ctx,
        BufferOptions {
            page_size: 512,
            frames: 32,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap();
    for i in 0..100 {
        tree.insert(KeyTuple::from(format!("PREFIX-{i:03}").into_bytes()), i)
            .await
            .unwrap();
    }
    for i in 0..100 {
        let query = KeyTuple::from(format!("prefix-{i:03}").into_bytes());
        assert_eq!(tree.get(&query).await.unwrap(), Some(i));
        let (stored, value) = tree.get_key_value(&query).await.unwrap().unwrap();
        assert_eq!(stored.as_slice(), format!("PREFIX-{i:03}").as_bytes());
        assert_eq!(value, i);
    }
    assert_eq!(
        tree.range((Bound::Unbounded, Bound::Unbounded))
            .await
            .unwrap()
            .len(),
        100
    );
}

#[tokio::test]
async fn cancelled_generic_patch_cannot_change_mapping_or_retained_image() {
    let tree = PagedBTree::new(context()).unwrap();
    tree.insert(key(1), 10).await.unwrap();
    let access = tree.buffer.access().await;
    let (pin, image, at, _) = tree.view_leaf(&access, key(1).as_slice()).await.unwrap();
    let id = super::view::NodeView::new(&image)
        .unwrap()
        .entry(at)
        .unwrap()
        .1 as u64;
    let bytes = image.to_vec();
    // Delay after descent/patch creation, before publication, using the generic lock.
    let held = tree.shard(id).lock().await;
    let mut update = tree.insert(key(1), 20).boxed_local();
    assert!(poll!(&mut update).is_pending());
    drop(update);
    let lookup = key(1);
    let mut remove = tree.remove(&lookup).boxed_local();
    assert!(poll!(&mut remove).is_pending());
    drop(remove);
    assert_eq!(&*pin.snapshot().await, bytes);
    drop(held);
    assert_eq!(tree.get(&key(1)).await.unwrap(), Some(10));
    tree.insert(key(1), 30).await.unwrap();
    assert_eq!(&*image, bytes);
    assert_eq!(tree.get(&key(1)).await.unwrap(), Some(30));
}

#[tokio::test]
async fn variable_keys_match_baseline_after_prefix_changes_deletes_and_restore() {
    use crate::btree::btree_index::BTreeIndex;
    let store = Arc::new(MemoryPageStore::default());
    let options = BufferOptions {
        page_size: 512,
        frames: 16,
        partitions: 1,
        ..BufferOptions::default()
    };
    let tree = PagedBTree::with_row_ids(context(), options, store.clone()).unwrap();
    let old = BTreeIndex::new(context());
    let keys = (0..120u64)
        .map(|id| {
            let mut bytes = vec![(id % 4) as u8; (id % 100) as usize];
            bytes.extend_from_slice(&id.to_be_bytes());
            KeyTuple::from(bytes)
        })
        .collect::<Vec<_>>();
    let mut seed = 773u64;
    for step in 0..1200u128 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let key = &keys[(seed >> 32) as usize % keys.len()];
        if seed & 3 == 0 {
            assert_eq!(tree.remove(key).await.unwrap(), old.remove(key).unwrap());
        } else {
            assert_eq!(
                tree.insert(key.clone(), u128::MAX - step).await.unwrap(),
                old.insert(key.clone(), u128::MAX - step).unwrap()
            );
        }
    }
    let state = tree.checkpoint().await.unwrap();
    let restored = PagedBTree::restore_row_ids(context(), options, store, &state)
        .await
        .unwrap();
    for key in &keys {
        assert_eq!(restored.get(key).await.unwrap(), old.get(key).unwrap());
    }
    let actual = restored
        .range((Bound::Unbounded, Bound::Unbounded))
        .await
        .unwrap();
    let expected = old.range((Bound::Unbounded, Bound::Unbounded)).unwrap();
    assert_eq!(actual.len(), expected.len());
    for ((ak, av), (ek, ev)) in actual.iter().zip(&expected) {
        assert_eq!(ak.as_slice(), ek.as_slice());
        assert_eq!(av, ev);
    }
}
