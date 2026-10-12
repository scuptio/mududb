//! Default length limits for character data types.

use crate::common::limitation;

/// Default maximum length in bytes for fixed-length character data (64 KiB).
pub const DT_CHAR_FIXED_LEN_DEFAULT: u64 = 64 * 1024; // 64KB

/// Default maximum length in bytes for variable-length character data.
pub const DT_CHAR_VAR_LEN_DEFAULT: u64 = limitation::DT_CHAR_VAR_LEN_MAX;
