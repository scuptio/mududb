use crate::universal::uni_record_type::UniRecordType;

use crate::universal::uni_result_set::UniResultSet;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniQueryResult {
    pub tuple_desc: UniRecordType,

    pub result_set: UniResultSet,
}

impl Default for UniQueryResult {
    fn default() -> Self {
        Self {
            tuple_desc: Default::default(),

            result_set: Default::default(),
        }
    }
}
