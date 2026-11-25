#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::{
    codec,
    node::{Node, Page},
    tests::{context, key},
    *,
};
use async_trait::async_trait;
use page_buffer::{
    AllocatorState, BufferAccess, BufferDomain, BufferManager, PageImage, PagePin, PageRef,
    PageReservation, ResidentPage,
};
use std::rc::Rc;
use std::{collections::HashSet, sync::Arc};

// Independent one-page backend: no ClockBufferManager, Frame, or eviction implementation.
struct SinglePageBuffer {
    domain: Rc<BufferDomain>,
    root: ResidentPage,
}
impl SinglePageBuffer {
    async fn new() -> Self {
        let domain = BufferDomain::new().unwrap();
        let access = domain.access().await;
        let image = PageImage::new(codec::encode(&Page::Node(Node::empty(0)), 512, 1).unwrap());
        let root = ResidentPage::new(domain.clone(), &access, domain.page_ref(1), image).unwrap();
        drop(access);
        Self { domain, root }
    }
}
#[async_trait(?Send)]
impl BufferManager for SinglePageBuffer {
    async fn access(&self) -> BufferAccess<'_> {
        self.domain.access().await
    }
    async fn maintenance(&self) -> BufferAccess<'_> {
        self.domain.maintenance().await
    }
    fn page_ref(&self, id: u64) -> PageRef {
        self.domain.page_ref(id)
    }
    fn try_pin<'a>(&self, access: &'a BufferAccess<'_>, page: &PageRef) -> RS<Option<PagePin<'a>>> {
        self.domain.validate_ref(page)?;
        if page.id() != 1 {
            return Err(invalid("single-page backend"));
        }
        self.root.pin(access).map(Some)
    }
    async fn pin<'a>(&self, access: &'a BufferAccess<'_>, page: &PageRef) -> RS<PagePin<'a>> {
        self.try_pin(access, page)?
            .ok_or_else(|| invalid("missing page"))
    }
    async fn reserve_page<'a>(&self, _: &'a BufferAccess<'_>) -> RS<PageReservation<'a>> {
        Err(invalid("single-page backend has no allocator"))
    }
    async fn initialize(
        &self,
        _: &BufferAccess<'_>,
        _: PageReservation<'_>,
        _: PageImage,
    ) -> RS<PageRef> {
        Err(invalid("single-page backend has no allocator"))
    }
    async fn prefetch(&self, _: &BufferAccess<'_>, _: &PageRef) {}
    async fn flush(&self, access: &BufferAccess<'_>) -> RS<()> {
        self.domain.validate(access, false)
    }
    fn allocator_state(&self, access: &BufferAccess<'_>) -> RS<AllocatorState> {
        self.domain.validate(access, false)?;
        Ok(AllocatorState { next_page: 2 })
    }
    async fn reclaim(&self, access: &BufferAccess<'_>, _: &HashSet<u64>) -> RS<usize> {
        self.domain.validate(access, true)?;
        Ok(0)
    }
    fn page_size(&self) -> usize {
        512
    }
    fn stats(&self) -> BufferStats {
        BufferStats::default()
    }
}

#[tokio::test]
async fn tree_runs_on_an_independent_dyn_backend() {
    let manager: Rc<dyn BufferManager> = Rc::new(SinglePageBuffer::new().await);
    let tree = PagedBTree::restore_with_buffer(
        context(),
        manager,
        &TreeCheckpoint {
            root: 1,
            next_page: 2,
            next_value: 1,
            count: 0,
            page_size: 512,
        },
    )
    .await
    .unwrap();
    for i in 0..6 {
        tree.insert(key(i), i as u128).await.unwrap();
    }
    assert_eq!(tree.get(&key(3)).await.unwrap(), Some(3));
    assert_eq!(tree.remove(&key(3)).await.unwrap(), Some(3));
    tree.checkpoint().await.unwrap();
    assert_eq!(tree.len().await.unwrap(), 5);
}

#[tokio::test]
async fn new_tree_can_inject_a_fresh_buffer_and_rejects_a_nonempty_space() {
    let manager: Rc<dyn BufferManager> = Rc::new(
        page_buffer::ClockBufferManager::new(
            BufferOptions::default(),
            Arc::new(MemoryPageStore::default()),
            AllocatorState { next_page: 1 },
            None,
        )
        .unwrap(),
    );
    let tree = PagedBTree::with_row_ids_buffer(context(), manager.clone())
        .await
        .unwrap();
    tree.insert(key(1), u128::MAX).await.unwrap();
    assert_eq!(tree.get(&key(1)).await.unwrap(), Some(u128::MAX));
    assert!(PagedBTree::<u128>::with_buffer(context(), manager)
        .await
        .is_err());
}

#[tokio::test]
async fn mbp1_crc32c_golden_image_restores_with_fixed_offsets() {
    let fixture = include_str!("fixtures/v1_leaf.hex")
        .lines()
        .filter(|line| !line.starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|s| u8::from_str_radix(s, 16).unwrap())
        .collect::<Vec<_>>();
    let page = codec::decode(&fixture, 512, 1).unwrap();
    assert_eq!(codec::encode(&page, 512, 1).unwrap(), fixture);
    let store = Arc::new(MemoryPageStore::default());
    store.write(1, &fixture).await.unwrap();
    let tree = PagedBTree::restore_row_ids(
        context(),
        BufferOptions {
            page_size: 512,
            frames: 8,
            partitions: 1,
            ..BufferOptions::default()
        },
        store,
        &TreeCheckpoint {
            root: 1,
            next_page: 2,
            next_value: 2,
            count: 1,
            page_size: 512,
        },
    )
    .await
    .unwrap();
    assert_eq!(
        tree.get(&key(7)).await.unwrap(),
        Some((9u128 << 64) | 0x11223344)
    );
    tree.insert(key(8), 8).await.unwrap();
    assert_eq!(tree.checkpoint().await.unwrap().count, 2);
}

#[tokio::test]
async fn validation_memo_never_accepts_a_replaced_corrupt_image() {
    let manager = Rc::new(SinglePageBuffer::new().await);
    let tree = PagedBTree::restore_with_buffer(
        context(),
        manager.clone(),
        &TreeCheckpoint {
            root: 1,
            next_page: 2,
            next_value: 1,
            count: 0,
            page_size: 512,
        },
    )
    .await
    .unwrap();
    assert_eq!(tree.get(&key(1)).await.unwrap(), None);
    {
        let access = manager.access().await;
        let pin = manager.pin(&access, &manager.page_ref(1)).await.unwrap();
        let mut corrupt = pin.snapshot().await.to_vec();
        corrupt[100] ^= 1;
        pin.write()
            .await
            .prepare(PageImage::new(corrupt))
            .unwrap()
            .publish();
    }
    assert!(tree.get(&key(1)).await.is_err());
}

#[tokio::test]
async fn valid_checksum_cannot_hide_a_bad_unvisited_slot_from_point_lookup() {
    let manager = Rc::new(SinglePageBuffer::new().await);
    let tree = PagedBTree::restore_with_buffer(
        context(),
        manager.clone(),
        &TreeCheckpoint {
            root: 1,
            next_page: 2,
            next_value: 1,
            count: 0,
            page_size: 512,
        },
    )
    .await
    .unwrap();
    for id in 0..6 {
        tree.insert(key(id), id as u128).await.unwrap();
    }
    assert_eq!(tree.get(&key(0)).await.unwrap(), Some(0));
    let access = manager.access().await;
    let pin = manager.pin(&access, &manager.page_ref(1)).await.unwrap();
    let snapshot = pin.snapshot().await;
    let view = super::view::NodeView::new(&snapshot).unwrap();
    let (last, _) = view.slot(view.count - 1).unwrap();
    let mut corrupt = snapshot.to_vec();
    corrupt[last + 24] ^= 0x80; // Last key length; lookup of the first key need not visit it.
    codec::seal(&mut corrupt);
    pin.write()
        .await
        .prepare(PageImage::new(corrupt))
        .unwrap()
        .publish();
    assert!(tree.get(&key(0)).await.is_err());
}
