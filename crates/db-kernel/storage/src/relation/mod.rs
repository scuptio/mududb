//! Relation storage: an MVCC row layer over two time-series files (keys and
//! values) with an in-memory index.

#![allow(clippy::module_inception)]
pub mod relation;

#[cfg(feature = "paged-index")]
mod catalog_actor;
/// Catalog access preserves the metadata service's message-sharing contract.
#[cfg(feature = "paged-index")]
pub use catalog_actor::CatalogActor as SharedCatalogRelation;
/// The original catalog implementation remains available without paged indexes.
#[cfg(not(feature = "paged-index"))]
pub use relation::Relation as SharedCatalogRelation;
