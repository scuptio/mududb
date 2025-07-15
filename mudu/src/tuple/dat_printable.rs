#[derive(Debug, Clone)]
pub struct DatPrintable {
    datum: String,
}

impl DatPrintable {
    pub fn from(s: String) -> DatPrintable {
        Self { datum: s }
    }

    pub fn str(&self) -> &String {
        &self.datum
    }

    pub fn into(self) -> String {
        self.datum
    }
}
