use ::catalog::contract::message_bus_api::Envelope;

#[derive(Debug)]
pub(crate) enum WorkerMailboxMsg {
    BusMessage(Envelope),
    Shutdown,
}
