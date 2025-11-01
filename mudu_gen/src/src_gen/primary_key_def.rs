#[derive(Debug, Clone)]
pub struct PrimaryKeyDef {
    primary_key: Vec<String>,
}

impl PrimaryKeyDef {
    pub fn new(primary_key: Vec<String>) -> Self {
        Self { primary_key }
    }

    pub fn primary_key(&self) -> &Vec<String> {
        &self.primary_key
    }
}
