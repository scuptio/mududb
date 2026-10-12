use crate::contract::timestamp::Timestamp;
use mudu::common::update_delta::UpdateDelta;

impl VersionDelta {
    /// Creates a delta that rewinds to the version stamped `timestamp`.
    pub fn new(timestamp: Timestamp, deleted: bool, update: Vec<UpdateDelta>) -> Self {
        Self {
            timestamp,
            deleted,
            update,
        }
    }

    /// Timestamp of the older version this delta reconstructs.
    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    /// Whether the reconstructed older version is a delete marker.
    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    /// The byte-level updates to undo.
    pub fn update_delta(&self) -> &Vec<UpdateDelta> {
        &self.update
    }

    /// Consumes the delta and returns the byte-level updates.
    pub fn update_delta_into(self) -> Vec<UpdateDelta> {
        self.update
    }
}

/// A delta that rewinds a tuple version to the immediately previous version.
pub struct VersionDelta {
    timestamp: Timestamp,
    deleted: bool,
    update: Vec<UpdateDelta>,
}
