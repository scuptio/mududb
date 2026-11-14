//! Index-side page interpretation; buffer backends never see Node or codec.
use super::{
    codec, invalid,
    node::{Link, Node, Page},
    PagedBTree, RS,
};
use crate::index_key::compare_context::CompareContext;
use page_buffer::{BufferAccess, BufferManager, PageImage, PagePin};
use std::rc::Rc;

impl<V: Send> PagedBTree<V> {
    /// Creates a tree on a fresh, exclusively owned buffer page-number space.
    /// The supplied manager must not already serve another tree in that space.
    pub async fn with_buffer(context: CompareContext, buffer: Rc<dyn BufferManager>) -> RS<Self> {
        codec::validate_size(buffer.page_size())?;
        let access = buffer.maintenance().await;
        if buffer.allocator_state(&access)?.next_page != 1 {
            return Err(invalid("new tree requires an empty buffer"));
        }
        let reserved = buffer.reserve_page(&access).await?;
        let image = PageImage::new(codec::encode(
            &Page::Node(Node::empty(0)),
            buffer.page_size(),
            reserved.id(),
        )?);
        let root = buffer.initialize(&access, reserved, image).await?.id();
        drop(access);
        Ok(Self::from_buffer(context, buffer, root))
    }
    /// Uses the optional resident fast path before requesting an asynchronous load.
    pub(super) async fn pin<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        link: &Link,
    ) -> RS<PagePin<'a>> {
        let reference = self.buffer.page_ref(link.pid);
        match self.buffer.try_pin(access, &reference)? {
            Some(pin) => Ok(pin),
            None => self.buffer.pin(access, &reference).await,
        }
    }
    /// Captures one fully validated immutable image without decoding its records.
    pub(super) async fn page_image<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        link: &Link,
    ) -> RS<(PagePin<'a>, PageImage)> {
        let page = self.pin(access, link).await?;
        let image = page.snapshot().await;
        self.validate_page(&image, link.pid)?;
        Ok((page, image))
    }

    /// Encodes the reserved ID before initialization makes the page reference usable.
    /// Failure leaves an unreachable reservation for a later reclamation pass.
    /// The new page is published resident-dirty without a backing write: the
    /// cache file is a rebuildable spill area, so admission only needs the
    /// image to reach the store before eviction or a checkpoint flush, both
    /// of which write back dirty frames.
    pub(super) async fn allocate(&self, access: &BufferAccess<'_>, page: Page) -> RS<Link> {
        let reservation = self.buffer.reserve_page(access).await?;
        let image = self.resident_image(codec::encode_resident(
            &page,
            self.buffer.page_size(),
            reservation.id(),
        )?);
        Ok(Link::new(
            self.buffer
                .initialize_resident(access, reservation, image)
                .await?
                .id(),
        ))
    }
    /// Attach this tree's policy so deferred writes retain the correct counters.
    fn resident_image(&self, bytes: Vec<u8>) -> PageImage {
        let image = PageImage::with_shared_writeback(bytes, self.writeback_policy.clone());
        image.mark_validated(&super::images::VALIDATOR);
        image
    }

    /// A certificate is bound to immutable bytes, with page ID/size checked separately.
    pub(super) fn validate_page(&self, image: &PageImage, pid: u64) -> RS<()> {
        super::images::validate_observed(
            image,
            self.buffer.page_size(),
            pid,
            self.diagnostics.as_deref(),
        )
    }

    pub(super) async fn decode_image(&self, image: &PageImage, pid: u64) -> RS<Page> {
        self.validate_page(image, pid)?;
        codec::decode_validated(image)
    }

    /// Builds the complete replacement before a writer enters its publication phase.
    pub(super) fn image(&self, pid: u64, node: Node) -> RS<PageImage> {
        Ok(self.resident_image(codec::encode_resident(
            &Page::Node(node),
            self.buffer.page_size(),
            pid,
        )?))
    }
}

impl PagedBTree<u128> {
    /// Creates an inline row-ID tree on a fresh injected buffer page space.
    pub async fn with_row_ids_buffer(
        context: CompareContext,
        buffer: Rc<dyn BufferManager>,
    ) -> RS<Self> {
        let mut tree = Self::with_buffer(context, buffer).await?;
        tree.inline = Some(super::tree::InlineCodec {
            encode: |id| *id,
            decode: |id| id,
        });
        Ok(tree)
    }
}
