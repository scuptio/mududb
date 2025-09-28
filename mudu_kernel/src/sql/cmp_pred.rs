#[derive(Debug, Clone)]
pub struct CmpPred {
    attr: String,
    value: String,
}

impl CmpPred {
    pub fn new() -> Self {
        Self {
            attr: "".to_string(),
            value: "".to_string(),
        }
    }
}
