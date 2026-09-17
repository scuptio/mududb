#[cfg(test)]
mod tests {
    use crate::error::ec::{ErrorCode, Severity};
    use std::error::Error;
    use std::io::ErrorKind;

    #[test]
    fn severity_variants_are_distinct() {
        assert_ne!(Severity::User, Severity::Internal);
        assert_ne!(Severity::Internal, Severity::Transient);
        assert_ne!(Severity::Transient, Severity::User);
        assert_eq!(format!("{:?}", Severity::User), "User");
    }

    #[test]
    fn error_code_roundtrips_through_u32_for_all_variants() {
        let mut count = 0;
        for code in ErrorCode::iter() {
            let value = code.to_u32();
            let decoded = ErrorCode::from_u32(value).expect("defined code must decode");
            assert_eq!(decoded, code);
            count += 1;
        }
        assert!(count > 50);
    }

    #[test]
    fn from_u32_returns_none_for_undefined_codes() {
        assert!(ErrorCode::from_u32(0).is_none());
        assert!(ErrorCode::from_u32(999).is_none());
        assert!(ErrorCode::from_u32(100_000).is_none());
    }

    #[test]
    fn display_and_message_use_strum_message() {
        assert_eq!(
            format!("{}", ErrorCode::PermissionDenied),
            "Permission denied"
        );
        assert_eq!(ErrorCode::NotFound.message(), "I/O entity not found");
        assert_eq!(ErrorCode::Internal.message(), "Internal error");
    }

    #[test]
    fn error_kind_roundtrips_for_mapped_codes() {
        let pairs = [
            (ErrorCode::NotFound, ErrorKind::NotFound),
            (ErrorCode::PermissionDenied, ErrorKind::PermissionDenied),
            (ErrorCode::InvalidData, ErrorKind::InvalidData),
            (ErrorCode::TimedOut, ErrorKind::TimedOut),
            (ErrorCode::OutOfMemory, ErrorKind::OutOfMemory),
        ];
        for (code, kind) in pairs {
            assert_eq!(code.error_kind(), Some(kind));
            assert_eq!(ErrorCode::from(kind), code);
        }
    }

    #[test]
    fn application_codes_have_no_error_kind() {
        assert!(ErrorCode::Internal.error_kind().is_none());
        assert!(ErrorCode::EntityNotFound.error_kind().is_none());
        assert!(ErrorCode::Database.error_kind().is_none());
    }

    #[test]
    fn unknown_error_kind_maps_to_uncategorized() {
        assert_eq!(
            ErrorCode::from(ErrorKind::NotSeekable),
            ErrorCode::Uncategorized
        );
        assert_eq!(
            ErrorKind::try_from(ErrorCode::Uncategorized).unwrap_err(),
            ()
        );
    }

    #[test]
    fn severity_classification_is_consistent() {
        assert_eq!(ErrorCode::InvalidArgument.severity(), Severity::User);
        assert_eq!(ErrorCode::EntityNotFound.severity(), Severity::User);
        assert_eq!(ErrorCode::TimedOut.severity(), Severity::Transient);
        assert_eq!(ErrorCode::NetworkDown.severity(), Severity::Transient);
        assert_eq!(ErrorCode::Internal.severity(), Severity::Internal);
        assert_eq!(ErrorCode::Decode.severity(), Severity::Internal);
    }

    #[test]
    fn from_raw_os_error_maps_known_errno() {
        assert_eq!(ErrorCode::from_raw_os_error(2), ErrorCode::NotFound);
        assert_eq!(
            ErrorCode::from_raw_os_error(13),
            ErrorCode::PermissionDenied
        );
        assert_eq!(
            ErrorCode::from_raw_os_error(111),
            ErrorCode::ConnectionRefused
        );
    }

    #[test]
    fn from_io_error_maps_known_errors() {
        let not_found = std::io::Error::new(ErrorKind::NotFound, "missing");
        assert_eq!(ErrorCode::from(&not_found), ErrorCode::NotFound);
        assert_eq!(ErrorCode::from(not_found), ErrorCode::NotFound);

        let invalid_data = std::io::Error::new(ErrorKind::InvalidData, "bad");
        assert_eq!(ErrorCode::from(&invalid_data), ErrorCode::InvalidData);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn from_io_error_prefers_raw_errno_on_linux() {
        // A Linux-specific raw errno should map directly even if the ErrorKind
        // would otherwise map to Uncategorized.
        let io_err = std::io::Error::from_raw_os_error(39); // ENOTEMPTY
        assert_eq!(ErrorCode::from(&io_err), ErrorCode::DirectoryNotEmpty);
    }

    #[test]
    fn error_code_implements_std_error() {
        let code = ErrorCode::Internal;
        assert!(code.source().is_none());
        assert!(format!("{}", code).contains("Internal"));
    }
}
