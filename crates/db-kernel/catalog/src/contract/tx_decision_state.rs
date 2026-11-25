//! Pure coordinator state transitions, shared by apply and crash recovery.
//!
//! Re-exported from [`super::tx_decision`] where the state and transition
//! now live (the trait methods return the state, so the two are kept in one
//! module to avoid a cycle).

pub use super::tx_decision::{transition, CoordinatorState};
