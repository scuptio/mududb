//! Test fixtures for the kernel [`catalog`] crate.
//!
//! Fixtures live in a dedicated crate consumed through `dev-dependencies` so
//! that `catalog` itself ships no test scaffolding. Only the items bound to
//! `catalog` by the orphan rules (`Arbitrary` impls) and the fuzz targets
//! stay behind `catalog`'s `test` feature.

#![warn(missing_docs)]

/// In-memory [`catalog::contract::meta_mgr::MetaMgr`] fixture.
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
pub mod test_meta_mgr;
