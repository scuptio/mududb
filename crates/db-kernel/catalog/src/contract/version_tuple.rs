use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;

/// One version of a tuple: payload bytes, commit timestamp, and deleted flag.
#[derive(Debug, Clone)]
pub struct VersionTuple {
    timestamp: Timestamp,
    buf: Buf,
    deleted: bool,
}

impl VersionTuple {
    /// Creates a live (non-deleted) version.
    pub fn new(timestamp: Timestamp, buf: Buf) -> VersionTuple {
        Self {
            timestamp,
            buf,
            deleted: false,
        }
    }

    /// Creates a delete-marker version (empty payload).
    pub fn new_delete(timestamp: Timestamp) -> VersionTuple {
        Self {
            timestamp,
            buf: Vec::new(),
            deleted: true,
        }
    }

    /// The commit timestamp of this version.
    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    /// Overwrites the commit timestamp.
    pub fn update_timestamp(&mut self, ts: Timestamp) {
        self.timestamp = ts;
    }

    /// Consumes the version and returns its commit timestamp.
    pub fn timestamp_into(self) -> Timestamp {
        self.timestamp
    }

    /// The payload bytes.
    pub fn tuple(&self) -> &Buf {
        &self.buf
    }

    /// Consumes the version and returns its payload bytes without copying.
    pub fn tuple_into(self) -> Buf {
        self.buf
    }

    /// Mutable access to the payload bytes.
    pub fn mut_tuple(&mut self) -> &mut Buf {
        &mut self.buf
    }

    /// Whether this version is a delete marker.
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }
}
