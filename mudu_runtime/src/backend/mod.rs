//! Backend server implementations, session handling and HTTP API hosting.

#![allow(clippy::module_inception)]
mod accept_handle_task;
/// Application manager traits.
pub mod app_mgr;
/// Core backend server entry points.
pub mod backend;
/// Configuration field metadata.
pub mod cfg_meta;
#[cfg(test)]
mod cfg_meta_test;
/// HTTP API serving and client traits.
pub mod http_api;
#[cfg(test)]
mod http_api_test;
mod incoming_session;
mod management_thread;
/// Mudu application manager implementation.
pub mod mudu_app_mgr;
/// MuduDB server configuration.
pub mod mududb_cfg;
mod session;
mod session_ctx;
#[cfg(test)]
mod session_ctx_test;
mod session_handle_task;
#[cfg(test)]
mod session_test;
#[cfg(all(test, target_os = "linux"))]
mod sql_async_client_test;
pub mod tokio_backend;
pub mod web_handle_task;
pub mod web_serve;

#[cfg(target_os = "linux")]
#[path = "linux/server_ur/mod.rs"]
/// Linux io_uring server backend.
pub mod server_ur;
