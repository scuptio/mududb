use mudu::channel::Envelope;

#[derive(Debug)]
pub(crate) enum WorkerMailboxMsg {
    BusMessage(Envelope),
    Shutdown,
}
