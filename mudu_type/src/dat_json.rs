use mudu::utils::json::JsonValue;
use std::ops;

pub struct DatJson {
    json: JsonValue,
}

impl DatJson {
    pub fn from(json: JsonValue) -> Self {
        Self { json }
    }

    pub fn as_json_value(&self) -> &JsonValue {
        &self.json
    }

    pub fn into_json_value(self) -> JsonValue {
        self.json
    }

    pub fn to_string(&self) -> String {
        self.json.to_string()
    }
}

impl AsRef<JsonValue> for DatJson {
    #[inline]
    fn as_ref(&self) -> &JsonValue {
        self.as_json_value()
    }
}

impl ops::Deref for DatJson {
    type Target = JsonValue;

    #[inline]
    fn deref(&self) -> &JsonValue {
        self.as_ref()
    }
}
