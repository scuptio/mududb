#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniPutResult {
    pub ok: bool,
}

impl Default for UniPutResult {
    fn default() -> Self {
        Self {
            ok: Default::default(),
        }
    }
}
