use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;

#[derive(Debug, Clone)]
pub struct VersionTuple {
    timestamp: Timestamp,
    buf: Buf,
}

impl VersionTuple {
    pub fn new(timestamp: Timestamp, buf: Buf) -> VersionTuple {
        Self { timestamp, buf }
    }
    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    pub fn update_timestamp(&mut self, ts: Timestamp) {
        self.timestamp = ts;
    }

    pub fn timestamp_into(self) -> Timestamp {
        self.timestamp
    }

    pub fn tuple(&self) -> &Buf {
        &self.buf
    }

    pub fn mut_tuple(&mut self) -> &mut Buf {
        &mut self.buf
    }
}
