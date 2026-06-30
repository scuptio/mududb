use crate::exports::mududb::component_shim::types;

pub fn shim(code: u32, message: impl Into<String>, location: impl Into<String>) -> types::Error {
    types::Error {
        code,
        message: message.into(),
        source: "component-shim".to_string(),
        location: location.into(),
    }
}

pub fn type_error(expected: &str) -> types::Error {
    shim(2, format!("value is not {expected}"), "types")
}

pub fn range(message: impl Into<String>) -> types::Error {
    shim(3, message, "system")
}

pub fn unsupported(message: impl Into<String>) -> types::Error {
    shim(4, message, "system")
}

pub fn from_mudu(error: mududb::mudu::error::MuduError) -> types::Error {
    types::Error {
        code: error.ec().to_u32(),
        message: error.message().to_string(),
        source: "mududb".to_string(),
        location: error.loc().to_string(),
    }
}
