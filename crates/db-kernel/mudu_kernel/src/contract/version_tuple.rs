use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;

#[derive(Debug, Clone)]
pub struct VersionTuple {
    timestamp: Timestamp,
    buf: Buf,
    deleted: bool,
}

impl VersionTuple {
    pub fn new(timestamp: Timestamp, buf: Buf) -> VersionTuple {
        Self {
            timestamp,
            buf,
            deleted: false,
        }
    }

    pub fn new_delete(timestamp: Timestamp) -> VersionTuple {
        Self {
            timestamp,
            buf: Vec::new(),
            deleted: true,
        }
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

    /// Consumes the version and returns its payload bytes without copying.
    pub fn tuple_into(self) -> Buf {
        self.buf
    }

    pub fn mut_tuple(&mut self) -> &mut Buf {
        &mut self.buf
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }
}
