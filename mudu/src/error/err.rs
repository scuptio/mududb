use crate::error::ec::EC;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::error::Error;
use std::fmt;
use std::sync::Arc;

/// Custom error type with error code, message, and optional source
#[derive(Debug, Clone)]
pub struct MError {
    ec: EC,
    msg: String,
    src: Option<Arc<dyn Error>>,
}

unsafe impl Send for MError {}

unsafe impl Sync for MError {}
impl MError {
    pub fn new_with_ec(ec: EC) -> Self {
        Self::new(ec, String::new(), None)
    }

    pub fn new_with_ec_msg<S: AsRef<str>>(ec: EC, msg: S) -> Self {
        Self::new(ec, msg, None)
    }

    pub fn new_with_ec_msg_src<S: AsRef<str>, E: Into<Box<dyn Error + 'static>>>(
        ec: EC,
        msg: S,
        src: E,
    ) -> Self {
        Self::new(ec, msg, Some(Arc::from(src.into())))
    }

    pub fn new<S: AsRef<str>>(ec: EC, msg: S, src: Option<Arc<dyn Error>>) -> Self {
        let msg_str = msg.as_ref();
        let final_msg = if msg_str.is_empty() {
            ec.message().to_string()
        } else {
            msg_str.to_string()
        };

        Self {
            ec,
            msg: final_msg,
            src,
        }
    }

    pub fn ec(&self) -> EC {
        self.ec
    }

    pub fn message(&self) -> &str {
        &self.msg
    }

    pub fn set_message(&mut self, msg: String) {
        self.msg = msg;
    }
}

impl fmt::Display for MError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl Error for MError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.src.as_deref()
    }
}

// Macros for convenient error creation
#[macro_export]
macro_rules! m_error {
    ($ec:expr) => {
        $crate::error::err::MError::new_with_ec($ec)
    };
    ($ec:expr, $msg:expr) => {
        $crate::error::err::MError::new_with_ec_msg($ec, $msg)
    };
    ($ec:expr, $msg:expr, $src:expr) => {
        $crate::error::err::MError::new_with_ec_msg_src($ec, $msg, $src)
    };
}

// Equality implementation (considers only error code and message)
impl PartialEq for MError {
    fn eq(&self, other: &Self) -> bool {
        self.ec == other.ec && self.msg == other.msg
    }
}

impl Eq for MError {}

impl Default for MError {
    fn default() -> Self {
        Self::new_with_ec(EC::Ok)
    }
}

// Auto-derived by compiler, no need for unsafe impls
// unsafe impl Sync for MError {}
// unsafe impl Send for MError {}

// Serde implementation
const STRUCT_NAME: &str = "MError";
const FIELD_COUNT: usize = 3;
const FIELD_CODE: &str = "code";
const FIELD_MSG: &str = "msg";
const FIELD_SRC: &str = "src";
const FIELDS: &[&str] = &[FIELD_CODE, FIELD_MSG, FIELD_SRC];

#[derive(Serialize, Deserialize)]
enum ErrorSource {
    MError(MError),
    Other(String),
    None,
}

impl ErrorSource {
    fn into_error_source(self) -> Option<Arc<dyn Error>> {
        match self {
            Self::MError(err) => Some(Arc::new(err)),
            Self::Other(msg) => Some(Arc::new(m_error!(EC::MuduError, msg))),
            Self::None => None,
        }
    }
}

impl Serialize for MError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(STRUCT_NAME, FIELD_COUNT)?;

        state.serialize_field(FIELD_CODE, &self.ec)?;
        state.serialize_field(FIELD_MSG, &self.msg)?;

        let src_field = match &self.src {
            Some(src) => match src.downcast_ref::<MError>() {
                Some(merr) => ErrorSource::MError(merr.clone()),
                None => ErrorSource::Other(src.to_string()),
            },
            None => ErrorSource::None,
        };
        state.serialize_field(FIELD_SRC, &src_field)?;

        state.end()
    }
}

impl<'de> Deserialize<'de> for MError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(STRUCT_NAME, FIELDS, MErrorVisitor)
    }
}

struct MErrorVisitor;

impl<'de> Visitor<'de> for MErrorVisitor {
    type Value = MError;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        write!(formatter, "struct {}", STRUCT_NAME)
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<MError, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let ec = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;

        let msg: String = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(1, &self))?;

        let src: ErrorSource = seq
            .next_element()?
            .ok_or_else(|| de::Error::invalid_length(2, &self))?;

        Ok(MError::new(ec, msg, src.into_error_source()))
    }

    fn visit_map<V>(self, mut map: V) -> Result<MError, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut ec: Option<EC> = None;
        let mut msg: Option<String> = None;
        let mut src: Option<Arc<dyn Error>> = None;

        while let Some(key) = map.next_key()? {
            match key {
                FIELD_CODE => {
                    if ec.is_some() {
                        return Err(de::Error::duplicate_field(FIELD_CODE));
                    }
                    ec = Some(map.next_value()?);
                }
                FIELD_MSG => {
                    if msg.is_some() {
                        return Err(de::Error::duplicate_field(FIELD_MSG));
                    }
                    msg = Some(map.next_value()?);
                }
                FIELD_SRC => {
                    src = map.next_value::<ErrorSource>()?.into_error_source();
                }
                _ => {
                    return Err(de::Error::unknown_field(key, FIELDS));
                }
            }
        }

        let ec = ec.ok_or_else(|| de::Error::missing_field(FIELD_CODE))?;
        let msg = msg.ok_or_else(|| de::Error::missing_field(FIELD_MSG))?;

        Ok(MError::new(ec, msg, src))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
    use crate::error::ec::{EC, ERROR_CODE_END_AT, ERROR_CODE_START_AT};
    use serde_json;

    #[test]
    fn test_m_error_creation() {
        for err_code in ERROR_CODE_START_AT + 1..ERROR_CODE_END_AT - 1 {
            let ec = EC::from_u32(err_code).unwrap();
            let error = m_error!(ec);

            // Test serialization/deserialization
            let vec = serialize_sized_to_vec(&error).unwrap();
            let (deserialized, len) = deserialize_sized_from::<MError>(&vec).unwrap();
            assert!(len < vec.len() as u64);
            assert_eq!(error, deserialized);

            // Test JSON serialization
            let json_string = serde_json::to_string(&error).unwrap();
            let from_json: MError = serde_json::from_str(&json_string).unwrap();
            assert_eq!(error, from_json);
        }
    }

    #[test]
    fn test_error_with_message() {
        let error = m_error!(EC::InternalErr, "test message");
        assert_eq!(error.message(), "test message");
        assert_eq!(error.ec(), EC::InternalErr);
    }

    #[test]
    fn test_error_with_source() {
        let source = m_error!(EC::InternalErr);
        let error = m_error!(EC::InternalErr, "with source", source);
        assert!(error.source().is_some());
    }
}
