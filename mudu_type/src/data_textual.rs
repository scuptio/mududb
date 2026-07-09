use std::ops;

#[derive(Debug, Clone)]
pub struct DataTextual {
    datum: String,
}

impl DataTextual {
    pub fn from(s: String) -> DataTextual {
        Self { datum: s }
    }

    pub fn as_str(&self) -> &str {
        &self.datum
    }

    pub fn into(self) -> String {
        self.datum
    }
}

impl AsRef<str> for DataTextual {
    #[inline]
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl ops::Deref for DataTextual {
    type Target = str;

    #[inline]
    fn deref(&self) -> &str {
        self.as_ref()
    }
}
