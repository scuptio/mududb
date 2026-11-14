use crate::{invalid, BufferAccess, BufferDomain, PageImage, RS};
use crate::{
    LocalReadGuard as ARwLockReadGuard, LocalRwLock as ARwLock,
    LocalWriteGuard as ARwLockWriteGuard,
};
use std::cell::Cell;
use std::ops::Deref;
use std::rc::Rc;
// The high bit excludes new pins during frame replacement; lower bits count pins.
pub(crate) const BUSY: u64 = 1 << 63;

/// Manager-bound logical page reference; never a raw memory pointer.
#[derive(Clone, Copy, Debug)]
pub struct PageRef {
    pub(crate) id: u64,
    pub(crate) owner: u64,
}
impl PageRef {
    /// Logical page number for encoding persistent references.
    pub fn id(&self) -> u64 {
        self.id
    }
}
/// An unpublished page-number reservation. Dropping it leaves an unreachable
/// orphan, recovered by a maintenance sweep; Drop never initiates I/O.
pub struct PageReservation<'a> {
    pub(crate) ticket: Rc<()>,
    pub(crate) lease: std::marker::PhantomData<&'a ()>,
    pub(crate) page: PageRef,
}
impl PageReservation<'_> {
    /// Page number to include in the encoded image before initialization.
    pub fn id(&self) -> u64 {
        self.page.id
    }
}
pub(crate) struct Frame {
    pub data: ARwLock<PageImage>,
    pub pid: Cell<u64>,
    // Changes on frame reuse so a stale residency hint cannot pin a different page.
    pub generation: Cell<u64>,
    pub state: Cell<u64>,
    pub hot: Cell<u64>,
    // Dirty epochs distinguish a flushed snapshot from writes published during I/O.
    pub changed: Cell<u64>,
    pub flushed: Cell<u64>,
}
impl Frame {
    pub fn new(size: usize) -> Self {
        Self {
            data: ARwLock::new(PageImage::new(vec![0; size])),
            pid: 0.into(),
            generation: 1.into(),
            state: 0.into(),
            hot: 0.into(),
            changed: 0.into(),
            flushed: 0.into(),
        }
    }
    // Pin before checking identity: eviction must not race the identity check.
    // On mismatch the temporary pin drops and returns its count automatically.
    pub fn pin<'a>(self: &Rc<Self>, pid: u64, generation: u64) -> Option<PagePin<'a>> {
        let state = self.state.get();
        if state >= BUSY - 1 {
            return None;
        }
        self.state.set(state + 1);
        let pin = PagePin {
            lease: std::marker::PhantomData,
            frame: self.clone(),
        };
        if self.pid.get() != pid || self.generation.get() != generation {
            return None;
        }
        if self.hot.get() == 0 {
            self.hot.set(1);
        }
        Some(pin)
    }
    pub fn changed(&self) {
        self.changed.set(self.changed.get() + 1);
    }
}
/// Pins one frame against reuse. Page contents require a separate read/write guard.
/// A pin cannot outlive the access epoch that acquired it:
/// ```compile_fail
/// use page_buffer::BufferManager;
/// async fn escape(pool: &dyn BufferManager) {
///     let pin;
///     {
///         let access = pool.access().await;
///         pin = pool.pin(&access, &pool.page_ref(1)).await.unwrap();
///     }
///     let _ = pin.snapshot().await;
/// }
/// ```
pub struct PagePin<'a> {
    pub(crate) lease: std::marker::PhantomData<&'a ()>,
    pub(crate) frame: Rc<Frame>,
}
impl PagePin<'_> {
    /// Logical identity of the pinned page.
    pub fn id(&self) -> u64 {
        self.frame.pid.get()
    }
    /// A token uniquely identifying this frame's current content version:
    /// (frame identity, residency generation, write epoch). The generation
    /// bumps every time the frame is reused for another page, and the write
    /// epoch bumps on every publication, so a consumer caching anything
    /// derived from the image (for example a decoded node) can never observe
    /// a stale entry when validating against the token.
    pub fn cache_token(&self) -> (usize, u64, u64) {
        (
            Rc::as_ptr(&self.frame) as usize,
            self.frame.generation.get(),
            self.frame.changed.get(),
        )
    }
    /// Attempts a shared latch without entering the asynchronous wait path.
    pub fn try_read(&self) -> Option<PageReadGuard<'_>> {
        self.frame.data.try_read().map(PageReadGuard)
    }
    /// Acquires a shared byte-image guard, waiting asynchronously only on contention.
    pub async fn read(&self) -> PageReadGuard<'_> {
        if let Some(guard) = self.try_read() {
            return guard;
        }
        PageReadGuard(self.frame.data.read().await)
    }
    /// Copies the immutable image handle without copying its bytes.
    pub async fn snapshot(&self) -> PageImage {
        self.read().await.0.clone()
    }
    /// Attempts an exclusive page guard without waiting.
    pub fn try_write(&self) -> Option<PageWriteGuard<'_>> {
        self.frame.data.try_write().map(|guard| PageWriteGuard {
            guard,
            frame: &self.frame,
        })
    }
    /// Waits asynchronously for exclusive access. Never hold another page guard while waiting.
    pub async fn write(&self) -> PageWriteGuard<'_> {
        if let Some(guard) = self.try_write() {
            return guard;
        }
        PageWriteGuard {
            guard: self.frame.data.write().await,
            frame: &self.frame,
        }
    }
}
impl Drop for PagePin<'_> {
    fn drop(&mut self) {
        self.frame.state.set(self.frame.state.get() - 1);
    }
}
/// Read-only view of a latched byte image.
pub struct PageReadGuard<'a>(ARwLockReadGuard<'a, PageImage>);
impl PageReadGuard<'_> {
    /// Borrows the latched image and its validation certificate without cloning it.
    pub fn image(&self) -> &PageImage {
        &self.0
    }
}
impl Deref for PageReadGuard<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}
/// Exclusive guard. Arbitrary mutable byte access is intentionally unavailable.
pub struct PageWriteGuard<'a> {
    pub(crate) guard: ARwLockWriteGuard<'a, PageImage>,
    pub(crate) frame: &'a Frame,
}
impl Deref for PageWriteGuard<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.guard
    }
}
impl<'a> PageWriteGuard<'a> {
    /// Borrows the exclusive image without creating a COW-triggering snapshot.
    pub fn image(&self) -> &PageImage {
        &self.guard
    }

    /// Validates the replacement before any multi-page publication starts.
    pub fn prepare(self, image: PageImage) -> RS<PreparedWrite<'a>> {
        if image.len() != self.guard.len() {
            return Err(invalid("wrong replacement page size"));
        }
        Ok(PreparedWrite {
            guard: self,
            image: Some(image),
        })
    }
}
/// Validated pending replacement, retaining its latch even after publication.
/// Dropping before publication leaves the old image untouched. Multi-page callers
/// prepare every replacement first, then publish without yielding between pages.
pub struct PreparedWrite<'a> {
    guard: PageWriteGuard<'a>,
    image: Option<PageImage>,
}
impl PreparedWrite<'_> {
    /// Infallibly replaces bytes and marks them dirty. No await, I/O, or latch release.
    pub fn publish(&mut self) {
        if let Some(image) = self.image.take() {
            *self.guard.guard = image;
            self.guard.frame.changed();
        }
    }
}
/// Safe resident-page building block for alternative buffer implementations.
/// Backends own lookup/eviction policy; this handle supplies common pin/latch semantics.
pub struct ResidentPage {
    domain: Rc<BufferDomain>,
    frame: Rc<Frame>,
}
impl ResidentPage {
    /// Creates an initialized dirty resident page within a validated access epoch.
    pub fn new(
        domain: Rc<BufferDomain>,
        access: &BufferAccess<'_>,
        page: PageRef,
        image: PageImage,
    ) -> RS<Self> {
        domain.validate(access, false)?;
        domain.validate_ref(&page)?;
        let mut frame = Frame::new(image.len());
        frame.data = ARwLock::new(image);
        frame.pid.set(page.id);
        frame.changed();
        Ok(Self {
            domain,
            frame: Rc::new(frame),
        })
    }
    /// Pins this page under its manager's access lease.
    pub fn pin<'a>(&self, access: &'a BufferAccess<'_>) -> RS<PagePin<'a>> {
        self.domain.validate(access, false)?;
        self.frame
            .pin(self.frame.pid.get(), 1)
            .ok_or_else(|| invalid("resident pin unavailable"))
    }
}
