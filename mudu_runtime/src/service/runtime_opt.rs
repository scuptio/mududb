use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeOpt {
    pub enable_p2: bool,
    pub enable_async: bool,
}

impl Default for RuntimeOpt {
    fn default() -> Self {
        Self {
            enable_p2: false,
            enable_async: false,
        }
    }
}