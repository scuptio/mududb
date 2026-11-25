//! Payload validation and packed edits are checked against the existing decoder,
//! not against another copy of the fast path's slot arithmetic.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    codec, images,
    node::{Entry, Key, Link, Node, Page},
    patch,
    view::{validate_layout, NodeView},
};
use page_buffer::PageImage;
use std::sync::Arc;

const KEYS: [&[u8]; 3] = [b"aaa0-long", b"aaa2", b"aaa4-even-longer"];

fn node_image() -> PageImage {
    let mut node = Node::empty(0);
    node.high = Some(Key::inline(b"zzzz"));
    node.right = Link::new(9);
    for (i, &key) in KEYS.iter().enumerate() {
        Arc::make_mut(&mut node.entries).push(Entry {
            key: Key::inline(key),
            target: Link::new(i as u64),
            upper: 7,
        });
    }
    PageImage::new(codec::encode(&Page::Node(node), 512, 3).unwrap())
}
fn check(image: &PageImage, expected: &[(Vec<u8>, u128)], revision: u64) {
    let persistent = image.writeback_bytes().unwrap();
    let image = &*persistent;
    codec::validate_image(image, 512, 3).unwrap();
    validate_layout(image).unwrap();
    let decoded = codec::decode(image, 512, 3).unwrap();
    let node = decoded.node().unwrap();
    assert_eq!(node.revision, revision);
    assert_eq!(node.right.pid, 9);
    assert_eq!(node.high.as_ref().unwrap().head, b"zzzz");
    let actual = node
        .entries
        .iter()
        .map(|e| {
            (
                e.key.head.clone(),
                u128::from(e.target.pid) | u128::from(e.upper) << 64,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
    // Search's split accessors must agree with the independent full decoder,
    // including compressed keys and values whose upper 64 bits are nonzero.
    let view = NodeView::new(image).unwrap();
    for (at, (key, value)) in expected.iter().enumerate() {
        assert_eq!(view.key(at).unwrap().owned().head, *key);
        assert_eq!(view.value(at).unwrap(), *value);
    }
    assert!(view.key(usize::MAX).is_err());
    assert!(view.value(view.len()).is_err());
}

#[test]
fn packed_edits_preserve_records_fences_and_old_snapshots() {
    // Cover beginning/middle/end insertion and every deletion position independently.
    for (at, &key) in [b"aaa/", b"aaa1", b"aaa3", b"aaa5"].iter().enumerate() {
        let original = node_image();
        let retained = original.to_vec();
        let mut expected = KEYS
            .iter()
            .enumerate()
            .map(|(i, k)| (k.to_vec(), (7u128 << 64) | i as u128))
            .collect::<Vec<_>>();
        expected.insert(at, (key.to_vec(), u128::MAX));
        let inserted = patch::insert(
            &NodeView::new(&original).unwrap(),
            at,
            &Key::inline(key),
            u128::MAX,
        )
        .unwrap()
        .unwrap();
        let inserted = images::resident(inserted.apply_to_copy(&original).unwrap());
        check(&inserted, &expected, 1);
        assert_eq!(&*original, retained);
        for remove_at in 0..expected.len() {
            let mut expected = expected.clone();
            expected.remove(remove_at);
            let removed = patch::remove(&NodeView::new(&inserted).unwrap(), remove_at).unwrap();
            let removed = images::resident(removed.apply_to_copy(&inserted).unwrap());
            check(&removed, &expected, 2);
        }
        let replaced = patch::replace(&NodeView::new(&inserted).unwrap(), at, 0).unwrap();
        let replaced = images::resident(replaced.apply_to_copy(&inserted).unwrap());
        expected[at].1 = 0;
        check(&replaced, &expected, 2);
    }
}

#[test]
fn malformed_unvisited_slots_and_key_descriptors_are_rejected() {
    let image = node_image();
    let node = NodeView::new(&image).unwrap();
    let slot = node.slots + node.count * 4 - 4;
    let (record, _) = node.slot(node.count - 1).unwrap();
    for offset in [slot, record + 24] {
        let mut damaged = image.to_vec();
        damaged[offset] ^= 0x80;
        codec::seal(&mut damaged); // A valid checksum must not hide malformed payload.
        codec::validate_image(&damaged, 512, 3).unwrap();
        assert!(validate_layout(&damaged).is_err());
    }
    assert!(patch::insert(&node, 1, &Key::inline(b"bbb"), 1)
        .unwrap()
        .is_none());
    assert!(patch::replace(&node, node.len(), 1).is_err());
    assert!(patch::remove(&node, node.len()).is_err());
    let mut exhausted = image.to_vec();
    exhausted[24..32].copy_from_slice(&u64::MAX.to_le_bytes());
    codec::seal(&mut exhausted);
    assert!(patch::replace(&NodeView::new(&exhausted).unwrap(), 0, 9).is_err());
}
