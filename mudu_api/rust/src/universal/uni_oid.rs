// object id

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[derive(Default)]
pub struct UniOid {
    // higher 64 bits
    pub h: u64,

    // lower 64 bits
    pub l: u64,
}

