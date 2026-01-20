#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniError {
    pub err_code: u32,

    pub err_msg: String,
}

impl Default for UniError {
    fn default() -> Self {
        Self {
            err_code: Default::default(),

            err_msg: Default::default(),
        }
    }
}
