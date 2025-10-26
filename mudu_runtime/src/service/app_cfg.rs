use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AppCfg {
    pub name: String,
    pub lang: String,
    pub version: String,
}