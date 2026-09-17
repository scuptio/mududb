#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    Accepted,
    RoutingPending,
    Active,
    WaitingComponent,
    WaitingStorage,
    Sending,
    Closing,
    Closed,
}
