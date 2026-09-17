//! TCP clients for the MuduDB wire protocol.
//!
//! Provides a synchronous client (`SyncClient`), an async trait-based client
//! (`AsyncClient` / `AsyncClientImpl`) and a JSON-facing wrapper
//! (`JsonClient`) used by the CLI.

#![allow(clippy::module_inception)]

pub mod async_client;
pub mod client;
pub mod json_client;
