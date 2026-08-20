#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniFsDirent {
    pub name: String,

    pub is_dir: bool,

    pub length: u64,
}
