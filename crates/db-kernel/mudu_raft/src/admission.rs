//! Revocable, business-independent proposal admission fences.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A shared admission fence checked by the protocol owner immediately before
/// accepting a proposal. Invalidating it fences certificates already queued by
/// a caller, without introducing business types into the consensus core.
#[derive(Clone, Debug)]
pub struct AdmissionGuard(Arc<AtomicBool>);

impl Default for AdmissionGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl AdmissionGuard {
    /// Creates a valid fence. Invalidated fences cannot be reactivated.
    pub fn new() -> Self {
        Self(Arc::new(AtomicBool::new(true)))
    }
    /// Permanently invalidates every clone, including queued proposals.
    pub fn invalidate(&self) {
        self.0.store(false, Ordering::Release);
    }
    /// Whether the original admission authority is still valid.
    pub fn is_valid(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

/// Captures the exact consensus prefix against which external stream catch-up
/// was verified. The owner rejects this proof if any log entry, configuration,
/// term or transport session changed before membership admission.
#[derive(Clone, Debug)]
pub struct MembershipFence {
    /// Leader term owning the proof.
    pub term: u64,
    /// Old and incoming configurations used for the proof.
    pub membership: crate::stream::MembershipVersion,
    /// Fully committed log prefix; no speculative suffix may be present.
    pub committed_index: u64,
    /// Revocable group and peer-session authorities.
    pub guards: Vec<AdmissionGuard>,
}
