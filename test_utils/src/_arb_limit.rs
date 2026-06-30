//! Limits used by arbitrary test data generation.

/// Maximum number of fields in an arbitrary tuple key.
pub const _ARB_MAX_TUPLE_KEY_FIELD: usize = 5;

/// Maximum number of fields in an arbitrary tuple value.
pub const _ARB_MAX_TUPLE_VALUE_FIELD: usize = 100;

/// Maximum size of an arbitrary datum.
pub const _ARB_MAX_DATUM_SIZE: usize = 100;

/// Maximum length of an arbitrary name.
pub const _ARB_MAX_NAME_LEN: usize = 100;

/// Maximum length of an arbitrary string.
pub const _ARB_MAX_STRING_LEN: usize = 100;

/// Maximum length of an arbitrary array.
pub const _ARB_MAX_ARRAY_LEN: usize = 100;
