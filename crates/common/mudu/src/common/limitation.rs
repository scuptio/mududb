//! Hard length limits for character data types.

/// Maximum length in bytes for fixed-length character data (64 MiB).
pub const DT_CHAR_FIXED_LEN_MAX: u64 = 64 * 1024 * 1024; // 64MB

/// Maximum length in bytes for variable-length character data (effectively unbounded).
pub const DT_CHAR_VAR_LEN_MAX: u64 = u64::MAX;
