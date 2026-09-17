use crate::contract::timestamp::Timestamp;
use mudu::common::update_delta::UpdateDelta;

impl VersionDelta {
    pub fn new(timestamp: Timestamp, deleted: bool, update: Vec<UpdateDelta>) -> Self {
        Self {
            timestamp,
            deleted,
            update,
        }
    }

    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
    }

    pub fn is_deleted(&self) -> bool {
        self.deleted
    }

    pub fn update_delta(&self) -> &Vec<UpdateDelta> {
        &self.update
    }

    pub fn update_delta_into(self) -> Vec<UpdateDelta> {
        self.update
    }
}

pub struct VersionDelta {
    timestamp: Timestamp,
    deleted: bool,
    update: Vec<UpdateDelta>,
}
