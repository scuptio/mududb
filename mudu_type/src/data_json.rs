use mudu::utils::json::JsonValue;
use std::fmt;
use std::ops;

pub struct DataJson {
    json: JsonValue,
}

impl DataJson {
    pub fn from(json: JsonValue) -> Self {
        Self { json }
    }

    pub fn as_json_value(&self) -> &JsonValue {
        &self.json
    }

    pub fn into_json_value(self) -> JsonValue {
        self.json
    }
}

impl fmt::Display for DataJson {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.json.fmt(f)
    }
}

impl AsRef<JsonValue> for DataJson {
    #[inline]
    fn as_ref(&self) -> &JsonValue {
        self.as_json_value()
    }
}

impl ops::Deref for DataJson {
    type Target = JsonValue;

    #[inline]
    fn deref(&self) -> &JsonValue {
        self.as_ref()
    }
}
