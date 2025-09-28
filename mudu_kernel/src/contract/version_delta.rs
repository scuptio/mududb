use crate::contract::timestamp::Timestamp;
use mudu::common::update_delta::UpdateDelta;

impl VersionDelta {
    pub fn new(timestamp: Timestamp, update: Vec<UpdateDelta>) -> Self {
        Self { timestamp, update }
    }

    pub fn timestamp(&self) -> &Timestamp {
        &self.timestamp
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
    update: Vec<UpdateDelta>,
}
