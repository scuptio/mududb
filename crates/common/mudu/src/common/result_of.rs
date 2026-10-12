//! Conversions from `Option` and `std::io::Result` into [`RS`].

use crate::common::result::RS;
use crate::error::ErrorCode;
use crate::error::MuduError;
use crate::mudu_error;

/// Converts `opt` into a `RS`, failing with an [`ErrorCode::InvalidState`]
/// error carrying `err_msg` when `opt` is `None`.
pub fn rs_option<T>(opt: Option<T>, err_msg: &str) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(mudu_error!(ErrorCode::InvalidState, err_msg)),
    }
}

/// Converts `opt` into a `RS`, building the error lazily with `fr` when `opt`
/// is `None`.
pub fn rs_of_opt<T, R: Fn() -> MuduError>(opt: Option<T>, fr: R) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(fr()),
    }
}

/// Converts a `std::io::Result` into a `RS`, mapping the error to the
/// corresponding I/O error code.
pub fn rs_io<T>(r: Result<T, std::io::Error>) -> RS<T> {
    match r {
        Ok(t) => Ok(t),
        Err(e) => Err(crate::error::others::io_error(e)),
    }
}

/// Converts a `std::io::Result` into a `RS`, mapping the error to the
/// corresponding I/O error code.
pub fn std_io_error<T>(_r: std::io::Result<T>) -> RS<T> {
    match _r {
        Ok(t) => Ok(t),
        Err(e) => Err(crate::error::others::io_error(e)),
    }
}
