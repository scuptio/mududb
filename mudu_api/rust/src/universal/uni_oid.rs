// object id

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct UniOid {
    // higher 64 bits
    pub h: u64,

    // lower 64 bits
    pub l: u64,
}

impl Default for UniOid {
    fn default() -> Self {
        Self {
            h: Default::default(),

            l: Default::default(),
        }
    }
}
