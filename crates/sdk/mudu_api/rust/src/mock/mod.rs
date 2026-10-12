#[cfg(feature = "mock-sqlite")]
mod fs;
#[cfg(feature = "mock-sqlite")]
mod kv;
#[cfg(feature = "mock-sqlite")]
mod sqlite;

#[cfg(feature = "mock-sqlite")]
pub use sqlite::MockSqliteMuduSysCall;

/// Placeholder type so [`crate::MockSqliteMuduSysCall`] resolves without the
/// `mock-sqlite` feature; the real emulation requires the feature.
#[cfg(not(feature = "mock-sqlite"))]
pub struct MockSqliteMuduSysCall;
