//! Format-independent residency, allocation, and writeback. Cold misses serialize
//! within a partition; resident hints are validated through frame pinning.
//! Page interpretation and reachability discovery remain the caller's responsibility.
use crate::LocalRwLock as AMutex;
use crate::{
    handles::{Frame, BUSY},
    invalid, AllocatorState, BufferAccess, BufferDomain, BufferManager, PageImage, PagePin,
    PageRef, PageReservation, PageStore, RS,
};
use async_trait::async_trait;
use std::cell::Cell;
use std::rc::Rc;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Fixed page-cache configuration. Values and query results have separate costs.
#[derive(Clone, Copy, Debug)]
pub struct BufferOptions {
    /// Encoded page size; defaults to 16 KiB.
    pub page_size: usize,
    /// Maximum number of resident frames (at least eight).
    pub frames: usize,
    /// Number of independently locked cold-path directories.
    pub partitions: usize,
    /// Enable generation-validated direct frame hints.
    pub swizzling: bool,
    /// Prefetch at most one right sibling during a range scan.
    pub prefetch: bool,
}
impl Default for BufferOptions {
    fn default() -> Self {
        Self {
            page_size: 16 * 1024,
            frames: 256,
            partitions: 4,
            swizzling: true,
            prefetch: true,
        }
    }
}

/// Cumulative page-cache counters, sampled without acquiring a latch.
#[derive(Default, Debug, Clone, Copy)]
pub struct BufferStats {
    /// Validated frame-hint hits.
    pub hint_hits: u64,
    /// Backing reads.
    pub reads: u64,
    /// Backing writes.
    pub writes: u64,
    /// Successful frame replacements.
    pub evictions: u64,
}

// Releases the eviction claim even if backing I/O fails or its future is cancelled.
struct Claim<'a>(&'a Frame);
impl Drop for Claim<'_> {
    fn drop(&mut self) {
        self.0.state.set(0);
    }
}
struct Partition {
    map: HashMap<u64, usize>,
    hand: usize,
}

/// Partition-local clock cache over format-independent byte pages.
/// Directory partitions coalesce I/O; they are not executor threads.
///
/// ```compile_fail
/// use page_buffer::ClockBufferManager;
/// fn requires_send<T: Send>() {}
/// requires_send::<ClockBufferManager>();
/// ```
pub struct ClockBufferManager {
    domain: Rc<BufferDomain>,
    // Weak tickets distinguish live reservations from abandoned initialization work.
    reservations: AMutex<HashMap<u64, std::rc::Weak<()>>>,
    // Bounded, lossy hints; collisions affect speed, never logical page identity.
    hint_table: Vec<Cell<u64>>,
    options: BufferOptions,
    frames: Vec<Rc<Frame>>,
    partitions: Vec<AMutex<Partition>>,
    store: Arc<dyn PageStore>,
    next: Cell<u64>,
    free: AMutex<Vec<u64>>,
    hints: Cell<u64>,
    reads: Cell<u64>,
    writes: Cell<u64>,
    evictions: Cell<u64>,
}

impl ClockBufferManager {
    /// Builds a cache with optional initial dirty page (normally a fresh root).
    /// Reopen supplies no initial page and the saved allocator high-water mark.
    pub fn new(
        options: BufferOptions,
        store: Arc<dyn PageStore>,
        state: AllocatorState,
        initial: Option<(u64, PageImage)>,
    ) -> RS<Self> {
        if options.page_size == 0 || state.next_page == 0 {
            return Err(invalid("invalid buffer size or allocator state"));
        }
        if let Some((pid, image)) = &initial {
            if *pid == 0 || *pid >= state.next_page || image.len() != options.page_size {
                return Err(invalid("invalid initial page"));
            }
        }
        if options.frames < 8
            || options.frames > u32::MAX as usize
            || options.partitions == 0
            || options.frames / options.partitions < 8
        {
            return Err(invalid(
                "buffer requires at least eight frames per partition",
            ));
        }
        let frames: Vec<_> = (0..options.frames)
            .map(|_| Rc::new(Frame::new(options.page_size)))
            .collect();
        let mut frames = frames;
        let initial_id = initial.as_ref().map(|(pid, _)| *pid);
        if let Some((pid, image)) = initial {
            let home = pid as usize % options.partitions;
            let mut frame = Frame::new(options.page_size);
            frame.data = crate::LocalRwLock::new(image);
            frame.pid.set(pid);
            frame.changed();
            frames[home] = Rc::new(frame);
        }
        let partitions = (0..options.partitions)
            .map(|p| {
                let mut map = HashMap::new();
                if let Some(pid) = initial_id {
                    if p == pid as usize % options.partitions {
                        map.insert(pid, p);
                    }
                }
                AMutex::new(Partition { map, hand: 0 })
            })
            .collect();
        Ok(Self {
            domain: BufferDomain::new()?,
            reservations: AMutex::new(HashMap::new()),
            hint_table: (0..options.frames * 2).map(|_| Cell::new(0)).collect(),
            options,
            frames,
            partitions,
            store,
            next: Cell::new(state.next_page),
            free: AMutex::new(Vec::new()),
            hints: Cell::new(0),
            reads: Cell::new(0),
            writes: Cell::new(0),
            evictions: Cell::new(0),
        })
    }

    async fn fix_cold<'a>(&self, link: &PageRef) -> RS<PagePin<'a>> {
        let home = link.id as usize % self.partitions.len();
        let mut partition = self.partitions[home].lock().await;
        if let Some(&index) = partition.map.get(&link.id) {
            return self.pin_hint(link, index);
        }
        // The async partition guard coalesces misses and owns the load. A
        // cancelled future drops Claim and leaves the old page/map intact.
        let index = self.victim(home, &mut partition)?;
        let frame = &self.frames[index];
        let _claim = Claim(frame);
        self.flush_frame(frame).await?;
        let bytes = self.store.read(link.id).await?;
        self.reads.set(self.reads.get() + 1);
        if bytes.len() != self.options.page_size {
            return Err(invalid("short page image"));
        }
        let page = PageImage::new(bytes);
        let mut data = frame.data.write().await;
        let old = frame.pid.get();
        let generation = frame.generation.get() + 1;
        if generation > u32::MAX as u64 {
            return Err(invalid("frame generation exhausted"));
        }
        // No await from here through publication and the initial pin.
        *data = page;
        partition.map.remove(&old);
        partition.map.insert(link.id, index);
        frame.pid.set(link.id);
        frame.generation.set(generation);
        frame.changed.set(0);
        frame.flushed.set(0);
        drop(data);
        drop(_claim);
        if old != 0 {
            self.evictions.set(self.evictions.get() + 1);
        }
        self.pin_hint(link, index)
    }

    fn pin_hint<'a>(&self, link: &PageRef, index: usize) -> RS<PagePin<'a>> {
        let frame = &self.frames[index];
        let generation = frame.generation.get();
        let pin = frame
            .pin(link.id, generation)
            .ok_or_else(|| invalid("invalid resident frame"))?;
        self.hint_table[link.id as usize % self.hint_table.len()]
            .set((generation << 32) | index as u64);
        Ok(pin)
    }

    /// Searches a bounded number of clock slots instead of waiting on caller-held pins.
    fn victim(&self, home: usize, partition: &mut Partition) -> RS<usize> {
        let count = self.frames.len().div_ceil(self.partitions.len());
        for _ in 0..count * 2 {
            let index = home + partition.hand % count * self.partitions.len();
            partition.hand = partition.hand.wrapping_add(1);
            let Some(frame) = self.frames.get(index) else {
                continue;
            };
            if frame.hot.replace(0) != 0 {
                continue;
            }
            if frame.state.get() == 0 {
                frame.state.set(BUSY);
                return Ok(index);
            }
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Storage,
            "index buffer exhausted: release pins or increase frames"
        ))
    }

    /// Writes an immutable snapshot without holding its page latch across backing I/O.
    /// Only the captured epoch becomes clean; a newer publication remains dirty.
    async fn flush_frame(&self, frame: &Frame) -> RS<()> {
        let (epoch, image) = {
            let page = frame.data.read().await;
            let epoch = frame.changed.get();
            if epoch == frame.flushed.get() {
                return Ok(());
            }
            (epoch, page.clone())
        };
        let persistent = image.writeback_bytes()?;
        self.store.write(frame.pid.get(), &persistent).await?;
        // Concurrent writes have a newer changed epoch and remain dirty.
        frame.flushed.set(epoch);
        self.writes.set(self.writes.get() + 1);
        Ok(())
    }
}

#[async_trait(?Send)]
impl BufferManager for ClockBufferManager {
    fn try_access(&self) -> Option<BufferAccess<'_>> {
        self.domain.try_access()
    }
    async fn access(&self) -> BufferAccess<'_> {
        self.domain.access().await
    }
    async fn maintenance(&self) -> BufferAccess<'_> {
        self.domain.maintenance().await
    }
    fn page_ref(&self, id: u64) -> PageRef {
        self.domain.page_ref(id)
    }
    fn try_pin<'a>(&self, access: &'a BufferAccess<'_>, link: &PageRef) -> RS<Option<PagePin<'a>>> {
        self.domain.validate(access, false)?;
        self.domain.validate_ref(link)?;
        if self.options.swizzling {
            let hint = self.hint_table[link.id as usize % self.hint_table.len()].get();
            if hint != 0 {
                if let Some(frame) = self.frames.get((hint & 0xffff_ffff) as usize) {
                    if let Some(pin) = frame.pin(link.id, hint >> 32) {
                        self.hints.set(self.hints.get() + 1);
                        return Ok(Some(pin));
                    }
                }
            }
        }
        let home = link.id as usize % self.partitions.len();
        if let Some(partition) = self.partitions[home].try_lock() {
            if let Some(&index) = partition.map.get(&link.id) {
                return self.pin_hint(link, index).map(Some);
            }
        }
        Ok(None)
    }
    async fn pin<'a>(&self, access: &'a BufferAccess<'_>, link: &PageRef) -> RS<PagePin<'a>> {
        if let Some(pin) = self.try_pin(access, link)? {
            return Ok(pin);
        }
        if link.id >= self.next.get()
            || self.reservations.lock().await.contains_key(&link.id)
            || self.free.lock().await.contains(&link.id)
        {
            return Err(invalid("page is not initialized"));
        }
        self.fix_cold(link).await
    }
    async fn reserve_page<'a>(&self, access: &'a BufferAccess<'_>) -> RS<PageReservation<'a>> {
        self.domain.validate(access, false)?;
        let mut reservations = self.reservations.lock().await;
        let mut free = self.free.lock().await;
        // Recycle reservations whose initialization was abandoned: a dead
        // weak ticket means the page was never published, hence is
        // unreachable and can be handed out again without waiting for a
        // maintenance sweep. Keeps allocation dense for callers whose
        // backing file validates pages contiguously at open time.
        let dead: Vec<u64> = reservations
            .iter()
            .filter(|(_, ticket)| ticket.strong_count() == 0)
            .map(|(pid, _)| *pid)
            .collect();
        for pid in dead {
            reservations.remove(&pid);
            free.push(pid);
        }
        let pid = match free.pop() {
            Some(pid) => pid,
            None => {
                let next = self.next.get();
                self.next.set(
                    next.checked_add(1)
                        .ok_or_else(|| invalid("page IDs exhausted"))?,
                );
                next
            }
        };
        drop(free);
        let reservation = self.domain.reservation(access, pid)?;
        reservations.insert(pid, Rc::downgrade(&reservation.ticket));
        Ok(reservation)
    }
    async fn initialize(
        &self,
        access: &BufferAccess<'_>,
        reservation: PageReservation<'_>,
        image: PageImage,
    ) -> RS<PageRef> {
        self.domain.validate(access, false)?;
        self.domain.validate_ref(&reservation.page)?;
        if image.len() != self.options.page_size
            || !self
                .reservations
                .lock()
                .await
                .get(&reservation.id())
                .is_some_and(|ticket| ticket.ptr_eq(&Rc::downgrade(&reservation.ticket)))
        {
            return Err(invalid("invalid page initialization"));
        }
        let persistent = image.writeback_bytes()?;
        self.store.write(reservation.id(), &persistent).await?;
        self.writes.set(self.writes.get() + 1);
        self.reservations.lock().await.remove(&reservation.id());
        Ok(reservation.page)
    }
    async fn initialize_resident(
        &self,
        access: &BufferAccess<'_>,
        reservation: PageReservation<'_>,
        image: PageImage,
    ) -> RS<PageRef> {
        self.domain.validate(access, false)?;
        self.domain.validate_ref(&reservation.page)?;
        let pid = reservation.id();
        {
            let reservations = self.reservations.lock().await;
            if image.len() != self.options.page_size
                || !reservations
                    .get(&pid)
                    .is_some_and(|ticket| ticket.ptr_eq(&Rc::downgrade(&reservation.ticket)))
            {
                return Err(invalid("invalid page initialization"));
            }
        }
        // Installs the image as a dirty resident frame without a backing
        // write: the caller's own redo log covers durability, and a later
        // flush or eviction performs the writeback. The victim's writeback
        // is the only fallible step; on error the reservation stays orphaned
        // and is recycled by a later reserve_page sweep.
        let home = pid as usize % self.partitions.len();
        let page = {
            let mut partition = self.partitions[home].lock().await;
            let index = self.victim(home, &mut partition)?;
            let frame = &self.frames[index];
            let _claim = Claim(frame);
            self.flush_frame(frame).await?;
            let old = frame.pid.get();
            let generation = frame.generation.get() + 1;
            if generation > u32::MAX as u64 {
                return Err(invalid("frame generation exhausted"));
            }
            // No await from here through publication and the dirty mark.
            let mut data = frame.data.write().await;
            *data = image;
            partition.map.remove(&old);
            partition.map.insert(pid, index);
            frame.pid.set(pid);
            frame.generation.set(generation);
            frame.changed.set(0);
            frame.flushed.set(0);
            frame.changed();
            drop(data);
            drop(_claim);
            if old != 0 {
                self.evictions.set(self.evictions.get() + 1);
            }
            reservation.page
        };
        // Lock order: never wait on `reservations` while holding a partition
        // (reclaim takes them in the opposite order).
        self.reservations.lock().await.remove(&pid);
        Ok(page)
    }
    fn allocator_state(&self, access: &BufferAccess<'_>) -> RS<AllocatorState> {
        self.domain.validate(access, false)?;
        Ok(AllocatorState {
            next_page: self.next.get(),
        })
    }
    fn page_size(&self) -> usize {
        self.options.page_size
    }

    fn frames(&self) -> usize {
        self.options.frames
    }

    // Caller owns the exclusive operation gate: no outstanding traversal,
    // overflow snapshot, allocation, prefetch, or flush can retain these IDs.
    async fn reclaim(&self, access: &BufferAccess<'_>, live: &HashSet<u64>) -> RS<usize> {
        self.domain.validate(access, true)?;
        let next = self.next.get();
        if live.iter().any(|pid| *pid == 0 || *pid >= next) {
            return Err(invalid("live page outside allocator"));
        }
        let mut reservations = self.reservations.lock().await;
        if reservations
            .values()
            .any(|ticket| ticket.strong_count() != 0)
            || live.iter().any(|pid| reservations.contains_key(pid))
        {
            return Err(invalid(
                "reclamation cannot discard active or reachable reservations",
            ));
        }
        let free: Vec<_> = (1..self.next.get())
            .filter(|pid| !live.contains(pid))
            .collect();
        for partition in &self.partitions {
            let mut partition = partition.lock().await;
            let dead: Vec<_> = partition
                .map
                .iter()
                .filter(|(pid, _)| !live.contains(pid))
                .map(|(pid, index)| (*pid, *index))
                .collect();
            for (pid, index) in dead {
                let frame = &self.frames[index];
                if frame.state.get() != 0 {
                    return Err(invalid("reclamation requires quiescent page pins"));
                }
                let mut page = frame.data.write().await;
                *page = PageImage::new(vec![0; self.options.page_size]);
                frame.pid.set(0);
                frame.changed.set(0);
                frame.flushed.set(0);
                partition.map.remove(&pid);
            }
        }
        self.store.reclaim(&free).await?;
        let count = free.len();
        reservations.clear();
        *self.free.lock().await = free;
        Ok(count)
    }

    async fn prefetch(&self, access: &BufferAccess<'_>, link: &PageRef) {
        if self.options.prefetch && link.id != 0 {
            // Best effort: demand loads still report I/O/capacity errors.
            if let Ok(pin) = self.pin(access, link).await {
                pin.frame.hot.set(0);
            }
        }
    }

    async fn writeback_dirty(&self, access: &BufferAccess<'_>) -> RS<()> {
        self.domain.validate(access, false)?;
        // The same partition-serialized sweep as flush, minus the durability
        // fsync: callers whose writes are redo-protected (WAL) run this from
        // background rounds and keep flush for points that must fsync.
        futures::future::try_join_all(self.partitions.iter().enumerate().map(
            |(home, partition)| async move {
                let _guard = partition.lock().await;
                for index in (home..self.frames.len()).step_by(self.partitions.len()) {
                    self.flush_frame(&self.frames[index]).await?;
                }
                Ok::<(), mudu::error::MuduError>(())
            },
        ))
        .await?;
        Ok(())
    }

    fn dirty_count(&self) -> usize {
        self.frames
            .iter()
            .filter(|frame| frame.pid.get() != 0 && frame.changed.get() != frame.flushed.get())
            .count()
    }

    async fn flush(&self, access: &BufferAccess<'_>) -> RS<()> {
        self.domain.validate(access, false)?;
        // At most one I/O per configured partition; no page latch crosses I/O.
        futures::future::try_join_all(self.partitions.iter().enumerate().map(
            |(home, partition)| async move {
                let _guard = partition.lock().await;
                for index in (home..self.frames.len()).step_by(self.partitions.len()) {
                    self.flush_frame(&self.frames[index]).await?;
                }
                Ok::<(), mudu::error::MuduError>(())
            },
        ))
        .await?;
        self.store.flush().await
    }

    fn stats(&self) -> BufferStats {
        BufferStats {
            hint_hits: self.hints.get(),
            reads: self.reads.get(),
            writes: self.writes.get(),
            evictions: self.evictions.get(),
        }
    }
}
