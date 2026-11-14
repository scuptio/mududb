//! Opt-in process-wide allocation traffic during the measured benchmark interval.
//! Runtime/harness allocations are included; this is not live memory or RSS.
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};

struct CountingAllocator;
#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;
static ENABLED: AtomicBool = AtomicBool::new(false);
static CALLS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);

fn record(size: usize, reallocated: bool) {
    if ENABLED.load(Relaxed) {
        CALLS.fetch_add(1, Relaxed);
        BYTES.fetch_add(size as u64, Relaxed);
        if reallocated {
            REALLOCS.fetch_add(1, Relaxed);
        }
    }
}

// SAFETY: Every allocation operation is forwarded unchanged to System. Counters
// use only atomics, allocate nothing and never modify the allocator's pointers.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record(layout.size(), false);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: The caller supplies the GlobalAlloc layout contract.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record(layout.size(), false);
        }
        ptr
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        // SAFETY: The pointer, old layout and new size come from GlobalAlloc.
        let ptr = unsafe { System.realloc(ptr, layout, size) };
        if !ptr.is_null() {
            record(size, true);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: The pointer and layout identify a live System allocation.
        unsafe { System.dealloc(ptr, layout) };
    }
}

pub(super) fn start() {
    CALLS.store(0, Relaxed);
    BYTES.store(0, Relaxed);
    REALLOCS.store(0, Relaxed);
    ENABLED.store(true, Relaxed);
}
pub(super) fn stop() -> (u64, u64, u64) {
    ENABLED.store(false, Relaxed);
    (
        CALLS.load(Relaxed),
        BYTES.load(Relaxed),
        REALLOCS.load(Relaxed),
    )
}
