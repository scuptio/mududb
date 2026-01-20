use std::hash::{Hash, Hasher};
use serde::{Deserialize, Serialize};
use crate::storage::page_id;

#[derive(Clone, Serialize, Deserialize)]
pub struct PageIndex {
    pub file_id:u64,
    pub page_id:u64,
}


impl PageIndex {
    pub fn invalid_index() -> Self {
        Self {
            file_id: page_id::INVALID_FILE_ID,
            page_id: page_id::INVALID_PAGE_ID,
        }
    }

    pub fn new(file_id:u64, page_id:u64) -> PageIndex {
        Self {file_id,page_id}
    }
}


impl PartialEq<Self> for PageIndex {
    fn eq(&self, other: &Self) -> bool {
        self.file_id == other.file_id && self.page_id == other.page_id
    }
}

impl Eq for PageIndex {

}

impl Hash for PageIndex {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.file_id.hash(state);
        self.page_id.hash(state);
    }
}