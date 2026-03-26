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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerEvent {
    Accepted,
    RoutedLocal,
    RoutedRemote,
    RequestDecoded,
    StoragePending,
    StorageComplete,
    ComponentPending,
    ComponentReady,
    ResponseQueued,
    PeerClosed,
    FatalError,
}

pub fn advance_state(state: ConnectionState, event: WorkerEvent) -> ConnectionState {
    match (state, event) {
        (ConnectionState::Accepted, WorkerEvent::Accepted) => ConnectionState::RoutingPending,
        (ConnectionState::RoutingPending, WorkerEvent::RoutedLocal) => ConnectionState::Active,
        (ConnectionState::RoutingPending, WorkerEvent::RoutedRemote) => ConnectionState::Closed,
        (ConnectionState::Active, WorkerEvent::RequestDecoded) => ConnectionState::WaitingStorage,
        (ConnectionState::WaitingStorage, WorkerEvent::StorageComplete) => ConnectionState::Sending,
        (ConnectionState::Active, WorkerEvent::ComponentPending) => {
            ConnectionState::WaitingComponent
        }
        (ConnectionState::WaitingComponent, WorkerEvent::ComponentReady) => {
            ConnectionState::Sending
        }
        (ConnectionState::Sending, WorkerEvent::ResponseQueued) => ConnectionState::Active,
        (_, WorkerEvent::PeerClosed) | (_, WorkerEvent::FatalError) => ConnectionState::Closing,
        (ConnectionState::Closing, _) => ConnectionState::Closed,
        _ => state,
    }
}
