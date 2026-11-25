//! Independent checks for deferred checksums, split policy, and lexical shortcuts.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::node::{Entry, Key, Link, Node};
use super::{
    tests::{context, key},
    *,
};
use std::sync::Arc;

fn reference_split(node: &Node, size: usize) -> Option<usize> {
    (1..node.entries.len())
        .filter_map(|mid| {
            let mut left = node.clone();
            left.entries = node.entries[..mid].to_vec().into();
            left.high = Some(node.entries[mid].key.clone());
            let mut right = Node::empty(node.level);
            right.high = node.high.clone();
            let start = mid + usize::from(!node.leaf());
            right.entries = node.entries[start..].to_vec().into();
            let weight = |n: &Node| {
                n.entries
                    .iter()
                    .map(|e| e.key.head.len() + 28)
                    .sum::<usize>()
            };
            (codec::fits(&left, size) && codec::fits(&right, size))
                .then(|| (mid, weight(&left).abs_diff(weight(&right))))
        })
        .min_by_key(|(_, delta)| *delta)
        .map(|(mid, _)| mid)
}
#[test]
fn split_summaries_match_exhaustive_encoder_sizing() {
    for level in [0, 1] {
        for seed in 0..30 {
            let mut node = Node::empty(level);
            for i in 0..(3 + seed) {
                let mut head = vec![7; 1 + (i * 17 + seed * 13) % 70];
                let last = head.len() - 1;
                head[last] = i as u8;
                Arc::make_mut(&mut node.entries).push(Entry {
                    key: Key {
                        len: head.len() as u64 + 100,
                        head,
                        overflow: 1000 + i as u64,
                    },
                    target: Link::new(i as u64 + 1),
                    upper: 0,
                });
            }
            node.high = Some(Key::inline(&[9; 37]));
            for size in [256, 512, 1024, 2048] {
                let expected = reference_split(&node, size);
                let actual = split_point::divide(&node, size);
                match expected {
                    None => assert!(actual.is_err()),
                    Some(mid) => {
                        let (left, separator, right) = actual.unwrap();
                        assert_eq!(left.entries.len(), mid);
                        assert_eq!(separator.head, node.entries[mid].key.head);
                        assert!(codec::fits(&left, size) && codec::fits(&right, size));
                        assert_eq!(
                            right.entries.len(),
                            node.entries.len() - mid - usize::from(level != 0)
                        );
                        if level != 0 {
                            assert_eq!(right.left.pid, node.entries[mid].target.pid);
                        }
                    }
                }
            }
        }
    }
}

#[tokio::test]
async fn deferred_checksum_is_sealed_on_checkpoint_and_raw_corruption_rejected() {
    let store = Arc::new(MemoryPageStore::default());
    let options = BufferOptions {
        page_size: 512,
        frames: 8,
        partitions: 1,
        ..BufferOptions::default()
    };
    let tree = PagedBTree::with_row_ids(context(), options, store.clone()).unwrap();
    tree.insert(key(1), 1).await.unwrap();
    tree.insert(key(1), u128::MAX).await.unwrap();
    let access = tree.access().await;
    let (_, resident) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    let pid = tree.root_link().pid;
    assert!(codec::validate_image(&resident, 512, pid).is_err());
    assert!(images::validate(&resident, 512, pid + 1).is_err());
    drop(access);
    let state = tree.checkpoint().await.unwrap();
    let bytes = store.read(pid).await.unwrap();
    codec::decode(&bytes, 512, pid).unwrap();
    let restored = PagedBTree::restore_row_ids(context(), options, store.clone(), &state)
        .await
        .unwrap();
    assert_eq!(restored.get(&key(1)).await.unwrap(), Some(u128::MAX));
    let mut bad = bytes;
    bad[100] ^= 1;
    assert!(images::validate(&page_buffer::PageImage::new(bad), 512, pid).is_err());
}

#[tokio::test]
async fn lexical_prefix_decides_without_loading_unavailable_overflow() {
    let tree = PagedBTree::<u128>::new(context())
        .unwrap()
        .with_lexicographic_keys();
    let access = tree.access().await;
    let stored = view::KeyView {
        prefix: b"abc",
        suffix: b"def",
        overflow: 99999,
        len: 2000,
    };
    assert_eq!(
        tree.compare_view(&access, stored, b"abz", false)
            .await
            .unwrap(),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        tree.compare_view(&access, stored, b"ab", true)
            .await
            .unwrap(),
        std::cmp::Ordering::Less
    );
    assert!(tree
        .compare_view(&access, stored, b"abcdef", false)
        .await
        .is_err());
}

#[tokio::test]
async fn lexical_streaming_matches_full_comparator_across_splits_and_evictions() {
    let options = BufferOptions {
        page_size: 512,
        frames: 8,
        partitions: 1,
        ..BufferOptions::default()
    };
    let tree = PagedBTree::with_row_ids(context(), options, Arc::new(MemoryPageStore::default()))
        .unwrap()
        .with_lexicographic_keys();
    let mut keys = Vec::new();
    for i in 0..40u8 {
        let mut bytes = vec![3; 1600];
        bytes[1500] = i;
        let key = crate::index_key::key_tuple::KeyTuple::from(bytes);
        tree.insert(key.clone(), i as u128).await.unwrap();
        keys.push(key);
    }
    for (i, key) in keys.iter().enumerate() {
        assert_eq!(tree.get(key).await.unwrap(), Some(i as u128));
        assert_eq!(
            tree.insert(key.clone(), u128::MAX).await.unwrap(),
            Some(i as u128)
        );
        assert_eq!(tree.remove(key).await.unwrap(), Some(u128::MAX));
    }
    assert!(tree.is_empty().await.unwrap());
}

#[tokio::test]
async fn inline_updates_drop_search_snapshot_before_preparing_edits() {
    let tree = PagedBTree::with_row_ids(
        context(),
        BufferOptions::default(),
        Arc::new(MemoryPageStore::default()),
    )
    .unwrap();
    tree.insert(key(1), 1).await.unwrap();
    let access = tree.access().await;
    let (_, image) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    let pointer = image.as_ptr();
    drop(image);
    tree.insert(key(1), u128::MAX).await.unwrap();
    let (_, retained) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    assert_eq!(
        pointer,
        retained.as_ptr(),
        "unique inline edit must reuse bytes"
    );
    tree.insert(key(1), 42).await.unwrap();
    let (_, latest) = tree.page_image(&access, &tree.root_link()).await.unwrap();
    assert_ne!(retained.as_ptr(), latest.as_ptr());
    assert_eq!(
        view::NodeView::new(&retained).unwrap().entry(0).unwrap().1,
        u128::MAX
    );
    assert_eq!(
        view::NodeView::new(&latest).unwrap().entry(0).unwrap().1,
        42
    );
}
