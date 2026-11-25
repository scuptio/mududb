//! Manager identity and reclamation epochs, independent of page format.
//! Shared leases protect traversals, including copied page IDs between pins;
//! an exclusive lease waits for those traversals before page numbers can be reused.
use crate::{invalid, PageRef, PageReservation, RS};
use crate::{
    LocalReadGuard as ARwLockReadGuard, LocalRwLock as ARwLock,
    LocalWriteGuard as ARwLockWriteGuard,
};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOMAIN: AtomicU64 = AtomicU64::new(1);

/// Shared lease/identity primitive for buffer backends. Contains no page format.
pub struct BufferDomain {
    pub(crate) id: u64,
    gate: ARwLock<()>,
}
/// A traversal or exclusive maintenance lease. Lifetimes are intentionally tied
/// to the owning manager; copied page IDs must not outlive their traversal lease.
pub struct BufferAccess<'a> {
    owner: u64,
    guard: AccessGuard<'a>,
}
enum AccessGuard<'a> {
    Read(ARwLockReadGuard<'a, ()>),
    Write(ARwLockWriteGuard<'a, ()>),
}
impl BufferDomain {
    /// Creates a distinct manager identity and grace-period gate.
    pub fn new() -> RS<Rc<Self>> {
        let id = NEXT_DOMAIN
            .try_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_add(1))
            .map_err(|_| invalid("buffer identities exhausted"))?;
        Ok(Rc::new(Self {
            id,
            gate: ARwLock::new(()),
        }))
    }
    /// Attempts a shared epoch without allocating a boxed asynchronous call.
    /// The underlying fair lock still excludes queued exclusive maintenance.
    pub fn try_access(&self) -> Option<BufferAccess<'_>> {
        self.gate.try_read().map(|guard| BufferAccess {
            owner: self.id,
            guard: AccessGuard::Read(guard),
        })
    }
    /// Enters a shared traversal epoch.
    pub async fn access(&self) -> BufferAccess<'_> {
        BufferAccess {
            owner: self.id,
            guard: AccessGuard::Read(self.gate.read().await),
        }
    }
    /// Exclusively enters maintenance after prior traversals finish.
    pub async fn maintenance(&self) -> BufferAccess<'_> {
        BufferAccess {
            owner: self.id,
            guard: AccessGuard::Write(self.gate.write().await),
        }
    }
    /// Creates a reference within this manager's page-number space.
    pub fn page_ref(&self, id: u64) -> PageRef {
        PageRef { id, owner: self.id }
    }
    /// Mints a reservation for a backend that has exclusively reserved `id`.
    /// The backend must keep that ID unpublished until initialization finishes.
    pub fn reservation<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        id: u64,
    ) -> RS<PageReservation<'a>> {
        self.validate(access, false)?;
        let page = self.page_ref(id);
        self.validate_ref(&page)?;
        Ok(PageReservation {
            ticket: Rc::new(()),
            page,
            lease: std::marker::PhantomData,
        })
    }
    /// Rejects initialization of another manager's reservation.
    pub fn validate_reservation(&self, page: &PageReservation<'_>) -> RS<()> {
        self.validate_ref(&page.page)
    }
    /// Validates lease ownership and, optionally, exclusive maintenance access.
    pub fn validate(&self, access: &BufferAccess<'_>, exclusive: bool) -> RS<()> {
        if access.owner != self.id || (exclusive && !access.is_exclusive()) {
            return Err(invalid("wrong buffer access lease"));
        }
        Ok(())
    }
    /// Validates that a page reference belongs to this manager.
    pub fn validate_ref(&self, page: &PageRef) -> RS<()> {
        if page.owner != self.id || page.id == 0 {
            return Err(invalid("wrong buffer page reference"));
        }
        Ok(())
    }
}
impl BufferAccess<'_> {
    /// Whether the lease excludes all other accesses.
    pub fn is_exclusive(&self) -> bool {
        match &self.guard {
            AccessGuard::Read(guard) => {
                let _ = &**guard;
                false
            }
            AccessGuard::Write(guard) => {
                let _ = &**guard;
                true
            }
        }
    }
}
