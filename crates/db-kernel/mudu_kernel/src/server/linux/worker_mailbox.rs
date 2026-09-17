use crate::server::message_bus_api::Envelope;

#[derive(Debug)]
pub(in crate::server) enum WorkerMailboxMsg {
    BusMessage(Envelope),
    Shutdown,
}
