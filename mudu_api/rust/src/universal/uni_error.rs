#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniError {
    pub err_code: u32,

    pub err_msg: String,

    #[serde(default)]
    pub err_src: String,

    #[serde(default)]
    pub err_loc: String,
}

impl Default for UniError {
    fn default() -> Self {
        Self {
            err_code: Default::default(),

            err_msg: Default::default(),

            err_src: Default::default(),

            err_loc: Default::default(),
        }
    }
}
