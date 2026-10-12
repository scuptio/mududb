use std::ops;

/// A textual datum: an owned string in the text format of a data type.
#[derive(Debug, Clone)]
pub struct DataTextual {
    datum: String,
}

impl DataTextual {
    /// Wraps a string into a textual datum.
    pub fn from(s: String) -> DataTextual {
        Self { datum: s }
    }

    /// Returns the datum as a string slice.
    pub fn as_str(&self) -> &str {
        &self.datum
    }

    /// Consumes the datum and returns the underlying string.
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
