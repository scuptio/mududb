//! Client connection lifecycle states.

/// Lifecycle state of one client connection handled by a worker loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// The connection has been accepted but is not set up yet.
    Accepted,
    /// The connection is waiting to be routed to its owning worker.
    RoutingPending,
    /// The connection is set up and actively serving requests.
    Active,
    /// The connection is blocked waiting for a procedure component call.
    WaitingComponent,
    /// The connection is blocked waiting for a storage operation.
    WaitingStorage,
    /// A response is being sent to the client.
    Sending,
    /// The connection is being shut down.
    Closing,
    /// The connection is fully closed.
    Closed,
}
