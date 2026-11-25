//! Random value and UUID generation.
pub use crate::imp::random::Uuid;

/// Generates a random version-4 UUID using the default system environment.
pub fn uuid_v4() -> Uuid {
    crate::default_env().random().uuid_v4()
}

/// Generates a random version-4 UUID and returns its hyphenated string form.
pub fn next_uuid_v4_string() -> String {
    uuid_v4().to_string()
}

#[cfg(test)]
mod tests {
    #[test]
    #[allow(clippy::unwrap_used)]
    fn generated_uuid_strings_have_v4_version_and_rfc_variant() {
        let text = super::next_uuid_v4_string();
        let parsed = uuid::Uuid::parse_str(&text).unwrap();
        assert_eq!(parsed.get_version_num(), 4);
        assert_eq!(parsed.get_variant(), uuid::Variant::RFC4122);
        assert_eq!(parsed.hyphenated().to_string(), text);
    }
}
