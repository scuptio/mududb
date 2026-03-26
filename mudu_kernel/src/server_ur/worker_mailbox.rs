use crate::server_ur::transferred_connection::TransferredConnection;

#[derive(Debug)]
pub(in crate::server_ur) enum WorkerMailboxMsg {
    AdoptConnection(TransferredConnection),
    Shutdown,
}
