//! libsql-based database connector: connection, transaction and result-set
//! drivers used by the runtime to reach external libsql/SQLite databases.

#![warn(missing_docs)]

pub mod db_connector;
mod db_libsql;
mod db_libsql_async;
