#[cfg(test)]
mod tests {
    use crate::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
    use crate::error::err::{ErrorSource, MuduError, ResultExt, StringError};
    use crate::error::{ErrorCode, Severity};
    use crate::{ensure, mudu_error};
    use serde::Serialize;
    use std::error::Error;
    use std::fmt::Write as _;
    use std::io::ErrorKind;
    use std::sync::Arc;

    // The backtrace-capture toggle is global mutable state; serialize tests that
    // inspect or mutate it so they do not race when running in parallel.
    // mudu cannot depend on mudu_sys, so we use the std mutex here and scope the
    // clippy allowance to this test-only helper.
    #[allow(clippy::disallowed_types)]
    fn backtrace_test_lock() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        LOCK.lock().unwrap()
    }

    fn all_error_codes() -> Vec<ErrorCode> {
        ErrorCode::iter().collect()
    }

    fn error_kind_codes() -> Vec<(ErrorKind, u32)> {
        vec![
            (ErrorKind::NotFound, 2),
            (ErrorKind::PermissionDenied, 13),
            (ErrorKind::ConnectionRefused, 111),
            (ErrorKind::ConnectionReset, 104),
            (ErrorKind::ConnectionAborted, 103),
            (ErrorKind::NotConnected, 107),
            (ErrorKind::AddrInUse, 98),
            (ErrorKind::AddrNotAvailable, 99),
            (ErrorKind::BrokenPipe, 32),
            (ErrorKind::AlreadyExists, 17),
            (ErrorKind::WouldBlock, 11),
            (ErrorKind::InvalidInput, 22),
            (ErrorKind::InvalidData, 1000),
            (ErrorKind::TimedOut, 110),
            (ErrorKind::WriteZero, 1001),
            (ErrorKind::Interrupted, 4),
            (ErrorKind::Unsupported, 95),
            (ErrorKind::UnexpectedEof, 1002),
            (ErrorKind::OutOfMemory, 12),
            (ErrorKind::Other, 1003),
        ]
    }

    #[cfg(target_os = "linux")]
    fn linux_errno_codes() -> Vec<(i32, ErrorCode)> {
        ErrorCode::iter()
            .filter(|code| code.to_u32() < 1000)
            .map(|code| (code.to_u32() as i32, code))
            .collect()
    }

    #[test]
    fn test_m_error_creation() {
        for ec in all_error_codes() {
            let error = mudu_error!(ec);

            let vec = serialize_sized_to_vec(&error).unwrap();
            let (deserialized, len) = deserialize_sized_from::<MuduError>(&vec).unwrap();
            assert!(len < vec.len() as u64);
            assert_eq!(error, deserialized);

            let json_string = serde_json::to_string(&error).unwrap();
            let from_json: MuduError = serde_json::from_str(&json_string).unwrap();
            assert_eq!(error, from_json);
        }
    }

    #[test]
    fn every_public_error_code_round_trips() {
        for ec in all_error_codes() {
            assert_eq!(ErrorCode::from_u32(ec.to_u32()), Some(ec));
        }
        assert_eq!(ErrorCode::from_u32(0), None);
        assert_eq!(ErrorCode::from_u32(50040), None);
    }

    #[test]
    fn every_error_kind_round_trips() {
        for (kind, expected_code) in error_kind_codes() {
            let ec = ErrorCode::from(kind);
            assert_eq!(ec.to_u32(), expected_code);
            assert_eq!(ErrorKind::try_from(ec), Ok(kind));
            #[cfg(target_os = "linux")]
            if expected_code < 1000 {
                assert_eq!(ErrorCode::from_raw_os_error(expected_code as i32), ec);
            }
        }
        assert_eq!(ErrorKind::try_from(ErrorCode::Internal), Err(()));
        assert_eq!(
            ErrorCode::from_raw_os_error(9),
            ErrorCode::BadFileDescriptor
        );
        assert_eq!(
            ErrorCode::from_raw_os_error(123_456),
            ErrorCode::Uncategorized
        );
        #[cfg(target_os = "linux")]
        for (errno, expected) in linux_errno_codes() {
            assert_eq!(ErrorCode::from_raw_os_error(errno), expected);
        }
    }

    #[test]
    fn test_error_with_message() {
        let error = mudu_error!(ErrorCode::Internal, "test message");
        assert_eq!(error.message(), "test message");
        assert_eq!(error.ec(), ErrorCode::Internal);
    }

    #[test]
    fn test_error_with_source() {
        let source = mudu_error!(ErrorCode::Internal);
        let error = mudu_error!(ErrorCode::Internal, "with source", source);
        assert!(error.source().is_some());
    }

    #[test]
    fn test_context_wraps_source() {
        let inner = mudu_error!(ErrorCode::NotFound, "missing file");
        let outer = inner.context("load config failed");
        assert_eq!(outer.ec(), ErrorCode::NotFound);
        assert_eq!(outer.message(), "load config failed");
        assert!(outer.source().is_some());
    }

    #[test]
    fn test_result_ext_context() {
        let result: Result<(), MuduError> = Err(mudu_error!(ErrorCode::NotFound, "missing"));
        let wrapped = result.context("read failed");
        assert_eq!(wrapped.unwrap_err().message(), "read failed");
    }

    #[test]
    fn test_bail_and_ensure() {
        fn maybe_fail(should_fail: bool) -> Result<(), MuduError> {
            ensure!(!should_fail, ErrorCode::InvalidArgument, "should not fail");
            Ok(())
        }
        assert!(maybe_fail(false).is_ok());
        assert_eq!(
            maybe_fail(true).unwrap_err().ec(),
            ErrorCode::InvalidArgument
        );
    }

    #[test]
    fn test_severity_classification() {
        assert_eq!(ErrorCode::InvalidArgument.severity(), Severity::User);
        assert_eq!(ErrorCode::TimedOut.severity(), Severity::Transient);
        assert_eq!(ErrorCode::Internal.severity(), Severity::Internal);
    }

    #[test]
    fn accessors_and_mutators_work() {
        let _guard = backtrace_test_lock();
        // Reset to the default so this test is independent of execution order.
        MuduError::set_capture_backtrace(cfg!(debug_assertions));

        let mut error = MuduError::new_with_ec(ErrorCode::Internal);
        assert!(error.loc().contains("err_test.rs"));
        assert_eq!(error.severity(), Severity::Internal);
        #[cfg(debug_assertions)]
        assert!(error.backtrace().is_some());
        #[cfg(not(debug_assertions))]
        assert!(error.backtrace().is_none());

        error.set_message("updated".to_string());
        assert_eq!(error.message(), "updated");
    }

    #[test]
    fn capture_backtrace_can_be_toggled_in_tests() {
        let _guard = backtrace_test_lock();
        let was_capturing = MuduError::capture_backtrace().is_some();

        MuduError::set_capture_backtrace(false);
        let disabled = MuduError::capture_backtrace();
        assert!(disabled.is_none());

        let error = MuduError::new_with_ec(ErrorCode::Internal);
        assert!(error.backtrace().is_none());

        MuduError::set_capture_backtrace(true);
        let enabled = MuduError::capture_backtrace();
        assert!(enabled.is_some());

        MuduError::set_capture_backtrace(was_capturing);
    }

    #[test]
    fn new_with_ec_msg_opt_src_accepts_some_and_none() {
        let src: Option<Arc<dyn Error + Send + Sync + 'static>> =
            Some(Arc::new(mudu_error!(ErrorCode::NotFound)));
        let with_src = MuduError::new_with_ec_msg_opt_src(ErrorCode::Internal, "msg", src);
        assert!(with_src.source().is_some());

        let without_src = MuduError::new_with_ec_msg_opt_src(ErrorCode::Internal, "msg", None);
        assert!(without_src.source().is_none());
    }

    #[test]
    fn display_and_debug_include_code_and_message() {
        let error = mudu_error!(ErrorCode::NotFound, "missing");
        let text = format!("{}", error);
        assert!(text.contains("missing"));
        assert!(text.contains("(code 2)"));
        assert!(text.contains("err_test.rs"));

        let debug = format!("{:?}", error);
        assert!(debug.contains("MuduError"));
    }

    #[test]
    fn same_kind_compares_code_and_message() {
        let a = mudu_error!(ErrorCode::Internal, "same");
        let b = mudu_error!(ErrorCode::Internal, "same");
        assert!(a.same_kind(&b));

        let c = mudu_error!(ErrorCode::Internal, "different");
        assert!(!a.same_kind(&c));
    }

    #[test]
    fn display_chain_follows_sources() {
        let inner = mudu_error!(ErrorCode::NotFound, "inner");
        let outer = inner.context("outer");
        let chain = outer.display_chain();
        assert!(chain.contains("outer"));
        assert!(chain.contains("inner"));
    }

    #[test]
    fn from_str_and_string_use_internal_code() {
        let from_str: MuduError = "plain message".into();
        assert_eq!(from_str.ec(), ErrorCode::Internal);
        assert_eq!(from_str.message(), "plain message");

        let from_string: MuduError = String::from("owned message").into();
        assert_eq!(from_string.ec(), ErrorCode::Internal);
        assert_eq!(from_string.message(), "owned message");
    }

    #[test]
    fn from_io_error_maps_kind() {
        let io_err = std::io::Error::new(ErrorKind::NotFound, "no file");
        let err: MuduError = io_err.into();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        assert!(err.message().contains("no file"));
        assert!(err.source().is_some());
    }

    #[test]
    fn from_fmt_error_maps_to_fmt_write() {
        struct FailingWriter;
        impl std::fmt::Write for FailingWriter {
            fn write_str(&mut self, _s: &str) -> std::fmt::Result {
                Err(std::fmt::Error)
            }
        }
        let mut writer = FailingWriter;
        let fmt_err = write!(writer, "x").unwrap_err();
        let err: MuduError = fmt_err.into();
        assert_eq!(err.ec(), ErrorCode::FmtWrite);
    }

    #[test]
    fn from_utf8_errors_map_to_invalid_utf8() {
        let bytes = vec![0xff_u8];
        let str_err = std::str::from_utf8(&bytes).unwrap_err();
        let err: MuduError = str_err.into();
        assert_eq!(err.ec(), ErrorCode::InvalidUtf8);

        let string_err = String::from_utf8(bytes).unwrap_err();
        let err: MuduError = string_err.into();
        assert_eq!(err.ec(), ErrorCode::InvalidUtf8);
    }

    #[test]
    fn from_parse_errors_map_to_parse() {
        let int_err = "abc".parse::<i32>().unwrap_err();
        let err: MuduError = int_err.into();
        assert_eq!(err.ec(), ErrorCode::Parse);

        let float_err = "abc".parse::<f64>().unwrap_err();
        let err: MuduError = float_err.into();
        assert_eq!(err.ec(), ErrorCode::Parse);

        let addr_err = "not-an-addr".parse::<std::net::SocketAddr>().unwrap_err();
        let err: MuduError = addr_err.into();
        assert_eq!(err.ec(), ErrorCode::Parse);

        let char_err = "ab".parse::<char>().unwrap_err();
        let err: MuduError = char_err.into();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    // We need std::sync::Mutex here because the conversion under test is
    // specifically from std::sync::PoisonError, which only its own Mutex can
    // produce.
    #[allow(clippy::disallowed_types)]
    fn from_poison_error_maps_to_mutex() {
        let mutex: Arc<std::sync::Mutex<()>> = Arc::new(std::sync::Mutex::new(()));
        let mutex_clone = Arc::clone(&mutex);
        let _ = std::panic::catch_unwind(move || {
            let _guard = mutex_clone.lock().unwrap();
            panic!("poison");
        });
        let poison_err = Arc::try_unwrap(mutex).unwrap().into_inner().unwrap_err();
        let err: MuduError = poison_err.into();
        assert_eq!(err.ec(), ErrorCode::Mutex);
    }

    #[test]
    fn from_serde_json_error_maps_to_parse() {
        let json_err = serde_json::from_str::<i32>("not a number").unwrap_err();
        let err: MuduError = json_err.into();
        assert_eq!(err.ec(), ErrorCode::Parse);
    }

    #[test]
    fn result_ext_with_context_evaluates_lazily() {
        let result: Result<(), MuduError> = Err(mudu_error!(ErrorCode::NotFound, "missing"));
        let wrapped = result.with_context(|| "lazy context");
        assert_eq!(wrapped.unwrap_err().message(), "lazy context");
    }

    #[test]
    fn result_ext_ec_context_overrides_code() {
        let result: Result<(), MuduError> = Err(mudu_error!(ErrorCode::NotFound, "missing"));
        let wrapped = result.ec_context(ErrorCode::Internal, "wrapped");
        let err = wrapped.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Internal);
        assert_eq!(err.message(), "wrapped");
        assert!(err.source().is_some());
    }

    #[test]
    fn error_source_round_trips_through_other() {
        let source = ErrorSource::Other("plain".to_string());
        let arc = source.into_error_source().unwrap();
        let err_src = MuduError::new_with_ec_msg_src(ErrorCode::Internal, "msg", arc).err_src();
        assert!(matches!(err_src, ErrorSource::Other(s) if s == "plain"));

        let display = format!("{}", StringError("plain".to_string()));
        assert_eq!(display, "plain");
    }

    #[test]
    fn error_source_json_helpers_roundtrip() {
        let err = MuduError::new_with_ec_msg(ErrorCode::NotFound, "not found");
        let source = ErrorSource::MuduError(err);
        let json = source.to_json_str();
        assert!(!json.is_empty());

        let decoded = ErrorSource::from_json_str(&json);
        assert!(matches!(decoded, ErrorSource::MuduError(_)));
    }

    #[test]
    fn error_source_mudu_error_is_recognized_by_err_src() {
        let inner = MuduError::new_with_ec_msg(ErrorCode::NotFound, "inner");
        let outer = inner.context("outer");
        let src = outer.err_src();
        assert!(matches!(src, ErrorSource::MuduError(ref e) if e.message() == "inner"));
    }

    #[test]
    fn display_includes_source_and_location() {
        let inner = MuduError::new_with_ec_msg(ErrorCode::NotFound, "inner");
        let outer = inner.context("outer");
        let text = format!("{}", outer);
        assert!(text.contains("outer (code 2)"));
        assert!(text.contains(": inner (code 2)"));
        assert!(text.contains("err_test.rs"));
    }

    #[test]
    fn display_without_backtrace_shows_location_only() {
        let error = MuduError::new(
            ErrorCode::Internal,
            "no bt",
            None,
            "custom:loc".to_string(),
            None,
        );
        let text = format!("{}", error);
        assert!(text.contains("no bt (code 50000)"));
        assert!(text.contains("\n  at custom:loc"));
        assert!(!text.contains("backtrace:"));
    }

    #[test]
    fn mudu_error_with_mudu_source_roundtrips_through_json() {
        let inner = MuduError::new_with_ec_msg(ErrorCode::NotFound, "inner");
        let outer = inner.context("outer");

        let json = serde_json::to_string(&outer).unwrap();
        let decoded: MuduError = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.message(), "outer");
        assert!(decoded.source().is_some());
    }

    #[test]
    fn error_source_from_json_str_recognizes_mudu_error_variant() {
        let json =
            r#"{"MuduError":{"code":"NotFound","msg":"inner","src":"None","loc":"inner.rs:1"}}"#;
        let source = ErrorSource::from_json_str(json);
        assert!(matches!(source, ErrorSource::MuduError(_)));
    }

    #[test]
    fn error_source_from_json_str_ignores_invalid_json() {
        let source = ErrorSource::from_json_str("not json");
        assert!(matches!(source, ErrorSource::None));
    }

    #[test]
    fn deserialize_expecting_rejects_non_struct() {
        let result: Result<MuduError, _> = serde_json::from_str("123");
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_seq_rejects_short_array() {
        // Empty sequence misses the error code.
        let result: Result<MuduError, _> = serde_json::from_str("[]");
        assert!(result.is_err());

        // Single element misses the message.
        let result: Result<MuduError, _> = serde_json::from_str(r#"["Internal"]"#);
        assert!(result.is_err());

        // Two elements miss the source field.
        let result: Result<MuduError, _> = serde_json::from_str(r#"["Internal","msg"]"#);
        assert!(result.is_err());

        // Three elements miss the location field.
        let result: Result<MuduError, _> = serde_json::from_str(r#"["Internal","msg",null]"#);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_map_rejects_unknown_field() {
        let json = r#"{"code":"Internal","msg":"x","loc":"l","extra":1}"#;
        let result: Result<MuduError, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_map_rejects_duplicate_code_field() {
        let json = r#"{"code":"Internal","code":"NotFound","msg":"x","loc":"l"}"#;
        let result: Result<MuduError, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_map_rejects_duplicate_msg_field() {
        let json = r#"{"code":"Internal","msg":"x","msg":"y","loc":"l"}"#;
        let result: Result<MuduError, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_map_rejects_missing_field() {
        let json = r#"{"code":"Internal","msg":"x"}"#;
        let result: Result<MuduError, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn mudu_error_roundtrips_through_messagepack_struct_map() {
        // rmp-serde defaults to arrays; use a struct-map config to exercise visit_map.
        let err = MuduError::new_with_ec_msg(ErrorCode::Internal, "mp map test");
        let mut buf = Vec::new();
        {
            let mut serializer = rmp_serde::Serializer::new(&mut buf).with_struct_map();
            err.serialize(&mut serializer).unwrap();
        }
        let deserialized: MuduError = rmp_serde::from_slice(&buf).unwrap();
        assert!(deserialized.same_kind(&err));
    }

    #[test]
    fn sized_serializer_serializes_mudu_error_via_sizer() {
        // Exercise MuduError::serialize monomorphized with rmp_serde::Serializer<Sizer>.
        use crate::common::serde_utils::Sizer;
        let err = MuduError::new_with_ec_msg(ErrorCode::Internal, "sizer test");
        let mut sizer = Sizer::new();
        {
            let mut serializer = rmp_serde::Serializer::new(&mut sizer);
            err.serialize(&mut serializer).unwrap();
        }
        assert!(sizer.size() > 0);
    }

    #[test]
    fn new_with_ec_msg_src_accepts_rmp_encode_error() {
        let rmp_err = rmp_serde::encode::Error::DepthLimitExceeded;
        let err: MuduError =
            MuduError::new_with_ec_msg_src(ErrorCode::Encode, "encode failed", rmp_err);
        assert_eq!(err.ec(), ErrorCode::Encode);
    }

    #[test]
    fn deserialize_map_rejects_missing_loc_via_messagepack() {
        #[derive(serde::Serialize)]
        struct Partial {
            code: ErrorCode,
            msg: String,
        }
        let partial = Partial {
            code: ErrorCode::Internal,
            msg: "no loc".into(),
        };
        let mut buf = Vec::new();
        {
            let mut ser = rmp_serde::Serializer::new(&mut buf).with_struct_map();
            partial.serialize(&mut ser).unwrap();
        }
        let result: Result<MuduError, _> = rmp_serde::from_slice(&buf);
        assert!(result.is_err());
    }

    #[test]
    fn deserialize_seq_rejects_too_few_elements_via_messagepack() {
        let data = (ErrorCode::Internal, "short".to_string());
        let bytes = rmp_serde::to_vec(&data).unwrap();
        let result: Result<MuduError, _> = rmp_serde::from_slice(&bytes);
        assert!(result.is_err());
    }
}
