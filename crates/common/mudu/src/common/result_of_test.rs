#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::common::result_of::{rs_io, rs_of_opt, rs_option, std_io_error};
    use crate::error::ErrorCode;
    use crate::mudu_error;

    #[test]
    fn test_rs_option_some() {
        assert_eq!(rs_option(Some(42), "missing").unwrap(), 42);
    }

    #[test]
    fn test_rs_option_none() {
        let result = rs_option(None::<i32>, "value is missing");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);
        assert!(err.message().contains("value is missing"));
    }

    #[test]
    fn test_rs_of_opt_some() {
        assert_eq!(
            rs_of_opt(Some(42), || mudu_error!(ErrorCode::Internal, "x")).unwrap(),
            42
        );
    }

    #[test]
    fn test_rs_of_opt_none() {
        let result = rs_of_opt(None::<i32>, || {
            mudu_error!(ErrorCode::Internal, "custom error")
        });
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Internal);
        assert!(err.message().contains("custom error"));
    }

    #[test]
    fn test_rs_io_ok() {
        assert_eq!(rs_io::<i32>(Ok(42)).unwrap(), 42);
    }

    #[test]
    fn test_rs_io_err() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let result = rs_io::<i32>(Err(io_err));
        assert!(result.is_err());
    }

    #[test]
    fn test_std_io_error_ok() {
        assert_eq!(std_io_error::<i32>(Ok(42)).unwrap(), 42);
    }

    #[test]
    fn test_std_io_error_err() {
        let io_err = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied");
        let result = std_io_error::<i32>(Err(io_err));
        assert!(result.is_err());
    }
}
