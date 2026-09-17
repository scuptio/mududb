#[cfg(test)]
mod tests {
    use crate::type_error::{TyEC, TyErr};
    use mudu::error::ErrorCode;
    use std::error::Error;
    use std::io;

    fn assert_same_ec(actual: TyEC, expected: TyEC) {
        assert_eq!(
            std::mem::discriminant(&actual),
            std::mem::discriminant(&expected)
        );
    }

    #[test]
    fn new_sets_error_code_and_message() {
        let err = TyErr::new(TyEC::TypeConvertFailed, "conversion failed".to_string());
        assert_same_ec(err.ec(), TyEC::TypeConvertFailed);
        assert_eq!(err.msg(), "conversion failed");
    }

    #[test]
    fn new_with_src_stores_source_for_conversion() {
        let source = io::Error::other("io failed");
        let err = TyErr::new_with_src(TyEC::FatalInternalError, "wrapped".to_string(), source);
        assert_same_ec(err.ec(), TyEC::FatalInternalError);
        assert_eq!(err.msg(), "wrapped");
        // The Error trait's default source() is not overridden, but to_m_err()
        // wraps the TyErr itself as the source of the resulting MuduError.
        assert!(err.to_m_err().source().is_some());
    }

    #[test]
    fn new_without_source_has_no_source() {
        let err = TyErr::new(TyEC::ParamParseError, "parse failed".to_string());
        assert!(err.source().is_none());
    }

    #[test]
    fn display_uses_debug_format() {
        let err = TyErr::new(TyEC::InsufficientSpace, "no space".to_string());
        let display = format!("{}", err);
        let debug = format!("{:?}", err);
        assert_eq!(display, debug);
        assert!(display.contains("TyErr"));
        assert!(display.contains("no space"));
    }

    #[test]
    fn to_m_err_maps_type_convert_failed() {
        let err = TyErr::new(TyEC::TypeConvertFailed, "bad cast".to_string());
        let m_err = err.to_m_err();
        assert_eq!(m_err.ec(), ErrorCode::TypeConversionFailed);
        assert_eq!(m_err.message(), "bad cast");
        assert!(m_err.source().is_some());
    }

    #[test]
    fn to_m_err_maps_param_parse_error() {
        let err = TyErr::new(TyEC::ParamParseError, "bad param".to_string());
        let m_err = err.to_m_err();
        assert_eq!(m_err.ec(), ErrorCode::TypeConversionFailed);
        assert_eq!(m_err.message(), "bad param");
    }

    #[test]
    fn to_m_err_maps_insufficient_space() {
        let err = TyErr::new(TyEC::InsufficientSpace, "too small".to_string());
        let m_err = err.to_m_err();
        assert_eq!(m_err.ec(), ErrorCode::InsufficientBufferSpace);
        assert_eq!(m_err.message(), "too small");
    }

    #[test]
    fn to_m_err_maps_fatal_internal_error() {
        let err = TyErr::new(TyEC::FatalInternalError, "boom".to_string());
        let m_err = err.to_m_err();
        assert_eq!(m_err.ec(), ErrorCode::FatalInternal);
        assert_eq!(m_err.message(), "boom");
    }

    #[test]
    fn error_trait_source_is_none_by_default() {
        let err = TyErr::new(TyEC::TypeConvertFailed, "x".to_string());
        assert!(err.source().is_none());
    }

    #[test]
    fn to_m_err_with_src_preserves_source_chain() {
        let source = io::Error::other("src");
        let err = TyErr::new_with_src(TyEC::InsufficientSpace, "x".to_string(), source);
        let m_err = err.to_m_err();
        let src = m_err.source().expect("expected a source error");
        assert!(src.to_string().contains("TyErr"));
    }
}
