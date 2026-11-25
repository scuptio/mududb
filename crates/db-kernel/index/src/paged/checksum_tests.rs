//! Exact work accounting for validation, deferred writeback and independent trees.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    node::{Node, Page},
    tests::{context, key},
    *,
};
use page_buffer::PageImage;
use std::sync::Arc;

fn tree() -> PagedBTree<u128> {
    PagedBTree::with_row_ids(
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

#[test]
fn validation_counts_actual_hashes_and_rejects_do_not_gain_a_certificate() {
    let tree = tree().with_diagnostics();
    let encoded = codec::encode(&Page::Node(Node::empty(0)), 512, 42).unwrap();
    let image = PageImage::new(encoded.clone());
    tree.validate_page(&image, 42).unwrap();
    tree.validate_page(&image.clone(), 42).unwrap();
    let stats = tree.performance_stats();
    assert_eq!(stats.checksum_calls, 1);
    assert_eq!(stats.checksum_bytes, 504);
    assert_eq!(stats.writeback_copy_bytes, 0);
    // Context rejection precedes hashing, even for a previously validated image.
    assert!(tree.validate_page(&image, 43).is_err());
    assert_eq!(tree.performance_stats().checksum_calls, 1);
    let mut corrupt = encoded;
    corrupt[100] ^= 1; // Free space is included in the checksum too.
    let corrupt = PageImage::new(corrupt);
    for _ in 0..2 {
        assert!(tree.validate_page(&corrupt, 42).is_err());
    }
    assert_eq!(tree.performance_stats().checksum_calls, 3);
    assert_eq!(tree.performance_stats().checksum_bytes, 3 * 504);
    let mut bad_header = image.to_vec();
    bad_header[0] = 0;
    assert!(tree.validate_page(&PageImage::new(bad_header), 42).is_err());
    assert_eq!(tree.performance_stats().checksum_calls, 3);
}

#[tokio::test]
async fn deferred_writeback_observes_late_activation_and_clean_flush_is_free() {
    let tree = tree();
    tree.insert(key(10), 10).await.unwrap();
    let retained = {
        let access = tree.access().await;
        let (_, image) = tree.page_image(&access, &tree.root_link()).await.unwrap();
        image
    };
    // The retained image was produced before diagnostics were enabled.
    let tree = tree.with_diagnostics();
    assert_eq!(tree.performance_stats().checksum_calls, 0);
    let persisted = retained.writeback_bytes().unwrap();
    codec::validate_image(&persisted, 512, tree.root_link().pid).unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 1);
    assert_eq!(tree.performance_stats().writeback_copy_bytes, 512);
    tree.flush_pages().await.unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 2);
    tree.flush_pages().await.unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 2);
    tree.insert(key(10), 11).await.unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 2);
    tree.flush_pages().await.unwrap();
    let stats = tree.performance_stats();
    assert_eq!(stats.checksum_calls, 3);
    assert_eq!(stats.checksum_bytes, 3 * 504);
    assert_eq!(stats.writeback_copy_bytes, 3 * 512);
    // Re-enabling is idempotent and cannot orphan already attached policy counters.
    let tree = tree.with_diagnostics();
    assert_eq!(tree.performance_stats().checksum_calls, 3);
    let retained_copy = retained.writeback_bytes().unwrap();
    assert_eq!(&*retained_copy, &*persisted);
    assert_eq!(tree.performance_stats().checksum_calls, 4);
}

#[tokio::test]
async fn diagnostics_are_per_tree_and_disabled_work_does_not_get_counted() {
    let a = tree().with_diagnostics();
    let b = tree().with_diagnostics();
    let disabled = tree();
    for tree in [&a, &b, &disabled] {
        tree.insert(key(1), 1).await.unwrap();
    }
    // The first access validates the initially sealed, uncertified root.
    assert_eq!(a.performance_stats().checksum_calls, 1);
    assert_eq!(b.performance_stats().checksum_calls, 1);
    a.flush_pages().await.unwrap();
    assert_eq!(a.performance_stats().checksum_calls, 2);
    assert_eq!(b.performance_stats().checksum_calls, 1);
    b.flush_pages().await.unwrap();
    disabled.flush_pages().await.unwrap();
    assert_eq!(a.performance_stats().checksum_calls, 2);
    assert_eq!(b.performance_stats().checksum_calls, 2);
    let stats = disabled.performance_stats();
    assert_eq!(stats.checksum_calls, 0);
    assert_eq!(stats.checksum_bytes, 0);
    assert_eq!(stats.checksum_ns, 0);
    assert_eq!(stats.writeback_copy_bytes, 0);
}

#[derive(Default)]
struct FailOnceStore {
    inner: MemoryPageStore,
    failed: std::sync::atomic::AtomicBool,
}
#[async_trait::async_trait]
impl PageStore for FailOnceStore {
    async fn read(&self, pid: u64) -> RS<Vec<u8>> {
        self.inner.read(pid).await
    }
    async fn write(&self, pid: u64, bytes: &[u8]) -> RS<()> {
        if !self.failed.swap(true, std::sync::atomic::Ordering::Relaxed) {
            return Err(invalid("injected store failure"));
        }
        self.inner.write(pid, bytes).await
    }
    async fn flush(&self) -> RS<()> {
        self.inner.flush().await
    }
}

#[tokio::test]
async fn failed_store_write_still_counts_encoding_work_and_retry_counts_again() {
    let tree = PagedBTree::with_row_ids(
        context(),
        BufferOptions {
            page_size: 512,
            frames: 16,
            partitions: 1,
            ..BufferOptions::default()
        },
        Arc::new(FailOnceStore::default()),
    )
    .unwrap()
    .with_diagnostics();
    tree.insert(key(1), 1).await.unwrap();
    // Root validation contributes one hash before any writeback is attempted.
    assert_eq!(tree.performance_stats().checksum_calls, 1);
    assert!(tree.flush_pages().await.is_err());
    assert_eq!(tree.performance_stats().checksum_calls, 2);
    assert_eq!(tree.performance_stats().writeback_copy_bytes, 512);
    tree.flush_pages().await.unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 3);
    assert_eq!(tree.performance_stats().checksum_bytes, 1512);
    assert_eq!(tree.performance_stats().writeback_copy_bytes, 1024);
    tree.flush_pages().await.unwrap();
    assert_eq!(tree.performance_stats().checksum_calls, 3);
}

/// Slow, independent oracle catches polynomial, initialization and segment errors.
fn reference_crc32c(bytes: impl Iterator<Item = u8>) -> u32 {
    let mut crc = u32::MAX;
    for byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ if crc & 1 != 0 { 0x82f63b78 } else { 0 };
        }
    }
    !crc
}

#[test]
fn crc32c_matches_standard_vector_and_segmented_reference() {
    assert_eq!(crc32c::crc32c(b""), 0);
    assert_eq!(crc32c::crc32c(b"123456789"), 0xe3069283);
    for size in [512, 4096, 16384, 32768, 65535] {
        let mut page: Vec<u8> = (0..size).map(|i| (i * 37 + i / 251) as u8).collect();
        let before = page.clone();
        let expected = reference_crc32c(page[..40].iter().chain(&page[48..]).copied());
        codec::seal(&mut page);
        assert_eq!(codec::u64_at(&page, 40), u64::from(expected));
        assert_eq!(&page[..40], &before[..40]);
        assert_eq!(&page[48..], &before[48..]);
        assert_eq!(&page[44..48], &[0; 4]);
        // Stale checksum bytes must have no effect on the next computation.
        page[40..48].fill(0xff);
        codec::seal(&mut page);
        assert_eq!(codec::u64_at(&page, 40), u64::from(expected));
    }
}

#[test]
fn crc32c_rejects_corruption_in_every_page_byte() {
    let encoded = codec::encode(&Page::Node(Node::empty(0)), 512, 42).unwrap();
    for offset in 0..encoded.len() {
        let mut corrupt = encoded.clone();
        corrupt[offset] ^= 1;
        assert!(
            codec::validate_image(&corrupt, 512, 42).is_err(),
            "offset {offset}"
        );
    }
}
