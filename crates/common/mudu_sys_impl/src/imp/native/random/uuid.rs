use std::fmt;
use std::ops::Deref;

/// A wrapper around `uuid::Uuid`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Uuid(::uuid::Uuid);

impl Uuid {
    pub fn new_v4() -> Self {
        Self(::uuid::Uuid::new_v4())
    }

    pub fn from_external(uuid: ::uuid::Uuid) -> Self {
        Self(uuid)
    }

    pub fn into_external(self) -> ::uuid::Uuid {
        self.0
    }

    pub fn from_u128(value: u128) -> Self {
        Self(::uuid::Uuid::from_u128(value))
    }

    pub fn as_u128(&self) -> u128 {
        self.0.as_u128()
    }
}

impl Deref for Uuid {
    type Target = ::uuid::Uuid;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl fmt::Debug for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl fmt::Display for Uuid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<::uuid::Uuid> for Uuid {
    fn from(value: ::uuid::Uuid) -> Self {
        Self::from_external(value)
    }
}

impl From<Uuid> for ::uuid::Uuid {
    fn from(value: Uuid) -> Self {
        value.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Uuid;

    #[test]
    fn from_u128_as_u128_roundtrip() {
        let value = 0x1234_5678_9abc_def0_1234_5678_9abc_def0_u128;
        let uuid = Uuid::from_u128(value);
        assert_eq!(uuid.as_u128(), value);
    }

    #[test]
    fn new_v4_is_non_zero() {
        let uuid = Uuid::new_v4();
        assert_ne!(uuid.as_u128(), 0);
    }

    #[test]
    fn display_is_hyphenated_hex() {
        let uuid = Uuid::from_u128(0x1234_5678_9abc_def0_1234_5678_9abc_def0_u128);
        let s = format!("{uuid}");
        assert_eq!(s.len(), 36);
        assert!(s.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        assert_eq!(s.matches('-').count(), 4);
    }

    #[test]
    fn from_external_into_external_roundtrip() {
        let external = ::uuid::Uuid::new_v4();
        let wrapped = Uuid::from_external(external);
        assert_eq!(wrapped.into_external(), external);
    }
}
