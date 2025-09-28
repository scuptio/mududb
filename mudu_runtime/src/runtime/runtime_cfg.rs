use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct RuntimeCfg {
    pub bytecode_path: String,
    pub ddl_path: String,
    pub listen_ip: String,
    pub listen_port: u16,
}

