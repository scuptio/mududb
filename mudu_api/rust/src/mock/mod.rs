#[cfg(feature = "mock-sqlite")]
mod sqlite;

#[cfg(feature = "mock-sqlite")]
pub use sqlite::MockSqliteMuduSysCall;

#[cfg(not(feature = "mock-sqlite"))]
pub struct MockSqliteMuduSysCall;
