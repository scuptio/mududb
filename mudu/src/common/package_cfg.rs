use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PackageCfg {
    pub name: String,
    pub lang: String,
    pub version: String,
    pub use_async: bool
}
