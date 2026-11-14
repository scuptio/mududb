//! Partition-local async B-link tree. Resident bytes use checked local borrows;
//! conflicting tasks wait asynchronously. Trees cannot cross executor threads.
mod checkpoint;
mod codec;
mod diagnostics;
mod images;
mod latch;
mod lexical;
mod maintenance;
mod mutations;
mod node;
mod node_cache;
mod operations;
mod pages;
mod patch;
mod resident;
mod resident_write;
mod search;
mod split;
mod split_point;
mod tree;
mod view;

pub use checkpoint::TreeCheckpoint;
pub use diagnostics::PerformanceStats;
pub use maintenance::MaintenanceStats;
pub use page_buffer::{
    BufferManager, BufferOptions, BufferStats, FilePageStore, MemoryPageStore, OrderedFile,
    PageStore,
};
pub use tree::PagedBTree;

use mudu::{common::result::RS, error::ErrorCode, mudu_error};

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Decode, message)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod bench;

#[cfg(test)]
mod maintenance_tests;

#[cfg(test)]
mod injection_tests;

#[cfg(test)]
mod bench_matrix;

#[cfg(test)]
mod fast_path_tests;
#[cfg(test)]
mod view_tests;

#[cfg(test)]
mod optimization_tests;

#[cfg(test)]
mod resident_tests;

#[cfg(test)]
mod batch_wait_tests;

#[cfg(test)]
mod checksum_tests;
