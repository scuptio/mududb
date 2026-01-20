use std::ops;

#[derive(Debug, Clone)]
pub struct DatTextual {
    datum: String,
}

impl DatTextual {
    pub fn from(s: String) -> DatTextual {
        Self { datum: s }
    }

    pub fn as_str(&self) -> &str {
        &self.datum
    }

    pub fn into(self) -> String {
        self.datum
    }
}

impl AsRef<str> for DatTextual {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl ops::Deref for DatTextual {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_ref()
    }
}
