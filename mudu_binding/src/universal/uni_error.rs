#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniError {
    pub err_code: u32,

    pub err_msg: String,

    #[serde(default)]
    pub err_src: String,

    #[serde(default)]
    pub err_loc: String,

    #[serde(default)]
    pub err_details: Vec<u8>,
}
