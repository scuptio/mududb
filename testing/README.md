# testing

Integration-test helpers for the MuduDB workspace. 
This crate provides reusable utilities for starting backend services, 
reserving ephemeral TCP ports, waiting for services to become ready, and connecting synchronous test clients.

## Responsibility

- Provide shared test fixtures and helper functions used by integration tests across the workspace.
- Reserve single and contiguous blocks of ephemeral TCP ports for test servers.
- Wait for backend and network services to signal readiness before running assertions.
- Connect `SyncClient` test clients with automatic retries.
- Offer a global domain lock to serialize tests that share runtime state.
- Create unique temporary directories for isolated test state.
- Start the debug server in a background thread for debugging tests.

## What does NOT belong here

- The actual database server implementation (see `mudu_runtime`, `mudu_kernel`, `mudu`).
- The synchronous CLI client itself (see `mudu_cli`).
- Low-level networking and system abstractions (see `mudu_sys`).
- User-facing contract and binding types (see `mudu_contract`, `mudu_binding`).
- General-purpose utility code not specific to testing (see `mudu_utils`).

## Main public entry points

- `testing::support` — common integration-test helpers.
  - `supports_server_mode` — checks whether the host can run a backend in a given `ServerMode`.
  - `is_permission_denied` — checks whether an error is a permission-denied I/O error.
  - `wait_until_backend_ready` — blocks until the backend readiness signal fires.
  - `test_runtime_domain_lock` — global mutex for serializing runtime-sharing tests.
  - `temp_dir` — creates a unique temporary directory.
  - `TestListener` — local TCP listener wrapper for port reservation.
  - `start_debug_server` — starts the debug server on a background thread.
- `reserve_port` — reserves a single ephemeral TCP port.
- `reserve_port_block` — reserves a contiguous block of TCP ports.
- `wait_until_port_ready` — waits until a service accepts TCP connections.
- `connect_sync_client_with_retry` — connects a `SyncClient` with retries.
