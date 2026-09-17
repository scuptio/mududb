use crate::error::ErrorCode;
use crate::error::MuduError;
use crate::mudu_error;

#[track_caller]
pub fn io_error(err: std::io::Error) -> MuduError {
    io_error_with_message(err, "io error")
}

/// Maps a `std::io::Error`, including its known raw errno, to an error code.
pub fn io_error_code(err: &std::io::Error) -> ErrorCode {
    ErrorCode::from(err)
}

#[track_caller]
pub fn io_error_with_message<S: AsRef<str>>(err: std::io::Error, message: S) -> MuduError {
    mudu_error!(io_error_code(&err), message, err)
}

/// Maps a network I/O error using the same exhaustive `ErrorKind` mapping.
pub fn network_error_code(err: &std::io::Error) -> ErrorCode {
    ErrorCode::from(err)
}

#[track_caller]
pub fn network_error_with_message<S: AsRef<str>>(err: std::io::Error, message: S) -> MuduError {
    mudu_error!(network_error_code(&err), message, err)
}

#[cfg(test)]
mod tests {
    use super::{
        io_error, io_error_code, io_error_with_message, network_error_code,
        network_error_with_message,
    };
    use crate::error::ErrorCode;
    use std::io::{Error, ErrorKind};

    #[test]
    fn maps_specific_io_error_kinds() {
        assert_eq!(
            io_error_code(&Error::from(ErrorKind::NotFound)),
            ErrorCode::NotFound
        );
        assert_eq!(
            io_error_code(&Error::from(ErrorKind::PermissionDenied)),
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            io_error_code(&Error::from(ErrorKind::ConnectionReset)),
            ErrorCode::ConnectionReset
        );
        assert_eq!(
            network_error_code(&Error::from(ErrorKind::ConnectionReset)),
            ErrorCode::ConnectionReset
        );
        assert_eq!(
            network_error_code(&Error::from(ErrorKind::ConnectionRefused)),
            ErrorCode::ConnectionRefused
        );
        assert_eq!(
            io_error_code(&Error::from_raw_os_error(9)),
            ErrorCode::BadFileDescriptor
        );
        assert_eq!(
            io_error_code(&Error::from_raw_os_error(123_456)),
            ErrorCode::Uncategorized
        );
    }

    #[test]
    fn io_and_network_helpers_build_errors() {
        let io_err = io_error(Error::from(ErrorKind::NotFound));
        assert_eq!(io_err.ec(), ErrorCode::NotFound);
        assert!(io_err.message().contains("io error"));

        let io_err = io_error_with_message(Error::from(ErrorKind::PermissionDenied), "read failed");
        assert_eq!(io_err.ec(), ErrorCode::PermissionDenied);
        assert!(io_err.message().contains("read failed"));

        let net_err =
            network_error_with_message(Error::from(ErrorKind::ConnectionRefused), "dial failed");
        assert_eq!(net_err.ec(), ErrorCode::ConnectionRefused);
        assert!(net_err.message().contains("dial failed"));
    }
}
