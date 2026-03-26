use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(super) struct ProcedureList {
    pub app_name: String,
    pub procedures: Vec<String>,
}
