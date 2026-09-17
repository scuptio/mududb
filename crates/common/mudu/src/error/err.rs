use crate::common::serde_utils;
use crate::error::{ErrorCode, Severity};
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::SerializeStruct;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};
use std::backtrace::Backtrace;
use std::error::Error;
use std::panic::Location;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Custom error type with error code, message, and optional source
#[derive(Debug, Clone)]
pub struct MuduError {
    ec: ErrorCode,
    msg: String,
    src: Option<Arc<dyn Error + Send + Sync + 'static>>,
    loc: String,
    backtrace: Option<Arc<Backtrace>>,
}

impl MuduError {
    #[track_caller]
    fn caller_location() -> String {
        let loc = Location::caller();
        format!("{}:{}", loc.file(), loc.line())
    }

    pub fn capture_backtrace() -> Option<Arc<Backtrace>> {
        // Capture backtraces in debug builds to keep release overhead low.
        if SHOULD_CAPTURE.load(Ordering::Relaxed) {
            Some(Arc::new(Backtrace::capture()))
        } else {
            None
        }
    }

    #[cfg(test)]
    pub fn set_capture_backtrace(enabled: bool) {
        SHOULD_CAPTURE.store(enabled, Ordering::Relaxed);
    }
}

static SHOULD_CAPTURE: AtomicBool = AtomicBool::new(cfg!(debug_assertions));

impl MuduError {
    #[track_caller]
    pub fn new_with_ec(ec: ErrorCode) -> Self {
        Self::new(
            ec,
            ec.message(),
            None,
            Self::caller_location(),
            Self::capture_backtrace(),
        )
    }

    #[track_caller]
    pub fn new_with_ec_msg<S: AsRef<str>>(ec: ErrorCode, msg: S) -> Self {
        Self::new(
            ec,
            msg.as_ref(),
            None,
            Self::caller_location(),
            Self::capture_backtrace(),
        )
    }

    #[track_caller]
    pub fn new_with_ec_msg_src<S: AsRef<str>, E: Into<Box<dyn Error + Send + Sync + 'static>>>(
        ec: ErrorCode,
        msg: S,
        src: E,
    ) -> Self {
        Self::new(
            ec,
            msg.as_ref(),
            Some(Arc::from(src.into())),
            Self::caller_location(),
            Self::capture_backtrace(),
        )
    }

    #[track_caller]
    pub fn new_with_ec_msg_opt_src<S: AsRef<str>>(
        ec: ErrorCode,
        msg: S,
        src: Option<Arc<dyn Error + Send + Sync + 'static>>,
    ) -> Self {
        Self::new(
            ec,
            msg.as_ref(),
            src,
            Self::caller_location(),
            Self::capture_backtrace(),
        )
    }

    pub fn new<S: AsRef<str>>(
        ec: ErrorCode,
        msg: S,
        src: Option<Arc<dyn Error + Send + Sync + 'static>>,
        loc: String,
        backtrace: Option<Arc<Backtrace>>,
    ) -> Self {
        Self {
            ec,
            msg: msg.as_ref().to_string(),
            src,
            loc,
            backtrace,
        }
    }

    pub fn ec(&self) -> ErrorCode {
        self.ec
    }

    pub fn message(&self) -> &str {
        &self.msg
    }

    pub fn loc(&self) -> &str {
        &self.loc
    }

    pub fn severity(&self) -> Severity {
        self.ec.severity()
    }

    pub fn backtrace(&self) -> Option<&Backtrace> {
        self.backtrace.as_deref()
    }

    pub fn set_message(&mut self, msg: String) {
        self.msg = msg;
    }

    /// Wraps this error with additional context, preserving the original as source.
    #[track_caller]
    pub fn context<S: AsRef<str>>(self, msg: S) -> Self {
        Self::new_with_ec_msg_src(self.ec, msg, self)
    }

    /// Returns true if both errors have the same code and message, ignoring location
    /// and source chain.
    pub fn same_kind(&self, other: &Self) -> bool {
        self.ec == other.ec && self.msg == other.msg
    }

    pub fn err_src(&self) -> ErrorSource {
        match &self.src {
            Some(src) => match src.downcast_ref::<MuduError>() {
                Some(merr) => ErrorSource::MuduError(merr.clone()),
                None => ErrorSource::Other(src.to_string()),
            },
            None => ErrorSource::None,
        }
    }

    /// Formats the full error chain for debugging.
    pub fn display_chain(&self) -> String {
        let mut s = format!("{} (code {})", self.msg, self.ec.to_u32());
        let mut source = self.source();
        while let Some(err) = source {
            s.push_str(&format!("\n  -> {}", err));
            source = err.source();
        }
        s
    }
}

impl std::fmt::Display for MuduError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} (code {})", self.msg, self.ec.to_u32())?;
        if let Some(src) = &self.src {
            write!(f, ": {}", src)?;
        }
        if let Some(bt) = &self.backtrace {
            write!(f, "\n  at {}\nbacktrace:\n{}", self.loc, bt)?;
        } else {
            write!(f, "\n  at {}", self.loc)?;
        }
        Ok(())
    }
}

impl Error for MuduError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.src.as_deref().map(|e| e as &(dyn Error + 'static))
    }
}

impl From<&str> for MuduError {
    #[track_caller]
    fn from(msg: &str) -> Self {
        Self::new_with_ec_msg(ErrorCode::Internal, msg)
    }
}

impl From<String> for MuduError {
    #[track_caller]
    fn from(msg: String) -> Self {
        Self::new_with_ec_msg(ErrorCode::Internal, msg)
    }
}

impl From<std::io::Error> for MuduError {
    #[track_caller]
    fn from(err: std::io::Error) -> Self {
        let ec = crate::error::others::io_error_code(&err);
        let msg = err.to_string();
        Self::new_with_ec_msg_src(ec, msg, err)
    }
}

impl From<std::fmt::Error> for MuduError {
    #[track_caller]
    fn from(err: std::fmt::Error) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::FmtWrite, err.to_string(), err)
    }
}

impl From<std::str::Utf8Error> for MuduError {
    #[track_caller]
    fn from(err: std::str::Utf8Error) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::InvalidUtf8, err.to_string(), err)
    }
}

impl From<std::string::FromUtf8Error> for MuduError {
    #[track_caller]
    fn from(err: std::string::FromUtf8Error) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::InvalidUtf8, err.to_string(), err)
    }
}

impl From<std::num::ParseIntError> for MuduError {
    #[track_caller]
    fn from(err: std::num::ParseIntError) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Parse, err.to_string(), err)
    }
}

impl From<std::num::ParseFloatError> for MuduError {
    #[track_caller]
    fn from(err: std::num::ParseFloatError) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Parse, err.to_string(), err)
    }
}

impl From<std::net::AddrParseError> for MuduError {
    #[track_caller]
    fn from(err: std::net::AddrParseError) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Parse, err.to_string(), err)
    }
}

impl From<std::char::ParseCharError> for MuduError {
    #[track_caller]
    fn from(err: std::char::ParseCharError) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Parse, err.to_string(), err)
    }
}

impl<T: Send + Sync + 'static> From<std::sync::PoisonError<T>> for MuduError {
    #[track_caller]
    fn from(err: std::sync::PoisonError<T>) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Mutex, err.to_string(), err)
    }
}

impl From<serde_json::Error> for MuduError {
    #[track_caller]
    fn from(err: serde_json::Error) -> Self {
        Self::new_with_ec_msg_src(ErrorCode::Parse, err.to_string(), err)
    }
}

// Macros for convenient error creation
#[macro_export]
macro_rules! mudu_error {
    ($ec:expr) => {
        $crate::error::MuduError::new_with_ec($ec)
    };
    ($ec:expr, $msg:expr) => {
        $crate::error::MuduError::new_with_ec_msg($ec, $msg)
    };
    ($ec:expr, $msg:expr, $src:expr) => {
        $crate::error::MuduError::new_with_ec_msg_src($ec, $msg, $src)
    };
}

/// Returns early with an [`MuduError`] using the given code and message.
#[macro_export]
macro_rules! bail {
    ($ec:expr, $msg:expr) => {
        return Err($crate::mudu_error!($ec, $msg));
    };
}

/// Returns early with an [`MuduError`] if the condition is false.
#[macro_export]
macro_rules! ensure {
    ($cond:expr, $ec:expr, $msg:expr) => {
        if !$cond {
            $crate::bail!($ec, $msg);
        }
    };
}

/// Extension trait for [`Result`] to attach context to errors.
pub trait ResultExt<T> {
    /// Wraps the error with the provided context message, preserving the original error.
    fn context<S: AsRef<str>>(self, msg: S) -> Result<T, MuduError>;

    /// Wraps the error with a lazily evaluated context message.
    fn with_context<S: AsRef<str>, F: FnOnce() -> S>(self, f: F) -> Result<T, MuduError>;

    /// Maps the error to the given [`ErrorCode`] with the provided message.
    fn ec_context<S: AsRef<str>>(self, ec: ErrorCode, msg: S) -> Result<T, MuduError>;
}

impl<T, E: Into<MuduError>> ResultExt<T> for Result<T, E> {
    #[track_caller]
    fn context<S: AsRef<str>>(self, msg: S) -> Result<T, MuduError> {
        self.map_err(|e| e.into().context(msg))
    }

    #[track_caller]
    fn with_context<S: AsRef<str>, F: FnOnce() -> S>(self, f: F) -> Result<T, MuduError> {
        self.map_err(|e| e.into().context(f()))
    }

    #[track_caller]
    fn ec_context<S: AsRef<str>>(self, ec: ErrorCode, msg: S) -> Result<T, MuduError> {
        self.map_err(|e| {
            let src: MuduError = e.into();
            MuduError::new_with_ec_msg_src(ec, msg, src)
        })
    }
}

// Equality implementation considers code, message, and location. The source chain is
// intentionally excluded because `dyn Error` does not implement `PartialEq`.
impl PartialEq for MuduError {
    fn eq(&self, other: &Self) -> bool {
        self.ec == other.ec && self.msg == other.msg && self.loc == other.loc
    }
}

impl Eq for MuduError {}

// Serde implementation
const STRUCT_NAME: &str = "MuduError";
const FIELD_COUNT: usize = 4;
const FIELD_CODE: &str = "code";
const FIELD_MSG: &str = "msg";
const FIELD_SRC: &str = "src";
const FIELD_LOC: &str = "loc";
const FIELDS: &[&str] = &[FIELD_CODE, FIELD_MSG, FIELD_SRC, FIELD_LOC];

/// Simple wrapper around a plain error message so that `ErrorSource::Other`
/// can be round-tripped without inventing a fake [`ErrorCode`] value.
#[derive(Debug, Clone)]
pub struct StringError(pub String);

impl std::fmt::Display for StringError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for StringError {}

#[derive(Serialize, Deserialize)]
pub enum ErrorSource {
    MuduError(MuduError),
    Other(String),
    None,
}

impl ErrorSource {
    pub fn into_error_source(self) -> Option<Arc<dyn Error + Send + Sync + 'static>> {
        match self {
            Self::MuduError(err) => Some(Arc::new(err)),
            Self::Other(msg) => Some(Arc::new(StringError(msg))),
            Self::None => None,
        }
    }

    pub fn from_json_str(s: &str) -> Self {
        let s = serde_utils::deserialize_from_json::<Self>(s);
        s.unwrap_or_else(|_| Self::None)
    }

    pub fn to_json_str(&self) -> String {
        let s = serde_utils::serialize_to_json(self);
        s.unwrap_or_default()
    }
}

impl Serialize for MuduError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut state = serializer.serialize_struct(STRUCT_NAME, FIELD_COUNT)?;

        state.serialize_field(FIELD_CODE, &self.ec)?;
        state.serialize_field(FIELD_MSG, &self.msg)?;

        let src_field = self.err_src();
        state.serialize_field(FIELD_SRC, &src_field)?;
        state.serialize_field(FIELD_LOC, &self.loc)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for MuduError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(STRUCT_NAME, FIELDS, MuduErrorVisitor)
    }
}

struct MuduErrorVisitor;

impl<'de> Visitor<'de> for MuduErrorVisitor {
    type Value = MuduError;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(formatter, "struct {}", STRUCT_NAME)
    }

    fn visit_seq<V>(self, mut seq: V) -> Result<MuduError, V::Error>
    where
        V: SeqAccess<'de>,
    {
        let ec = seq
            .next_element()?
            .ok_or(de::Error::invalid_length(0, &self))?;

        let msg: String = seq
            .next_element()?
            .ok_or(de::Error::invalid_length(1, &self))?;

        let src: ErrorSource = seq
            .next_element()?
            .ok_or(de::Error::invalid_length(2, &self))?;
        let loc: String = seq
            .next_element()?
            .ok_or(de::Error::invalid_length(3, &self))?;
        Ok(MuduError::new(
            ec,
            msg,
            src.into_error_source(),
            loc,
            MuduError::capture_backtrace(),
        ))
    }

    fn visit_map<V>(self, mut map: V) -> Result<MuduError, V::Error>
    where
        V: MapAccess<'de>,
    {
        let mut ec: Option<ErrorCode> = None;
        let mut msg: Option<String> = None;
        let mut src: Option<Arc<dyn Error + Send + Sync + 'static>> = None;
        let mut loc: Option<String> = None;
        while let Some(key) = map.next_key()? {
            match key {
                FIELD_CODE => {
                    if ec.is_some() {
                        return Err(de::Error::duplicate_field(FIELD_CODE));
                    }
                    ec = Some(map.next_value()?);
                }
                FIELD_MSG => {
                    if msg.is_some() {
                        return Err(de::Error::duplicate_field(FIELD_MSG));
                    }
                    msg = Some(map.next_value()?);
                }
                FIELD_SRC => {
                    src = map.next_value::<ErrorSource>()?.into_error_source();
                }
                FIELD_LOC => {
                    loc = Some(map.next_value::<String>()?);
                }
                _ => {
                    return Err(de::Error::unknown_field(key, FIELDS));
                }
            }
        }

        let ec = ec.ok_or(de::Error::missing_field(FIELD_CODE))?;
        let msg = msg.ok_or(de::Error::missing_field(FIELD_MSG))?;
        let loc = loc.ok_or(de::Error::missing_field(FIELD_LOC))?;
        Ok(MuduError::new(
            ec,
            msg,
            src,
            loc,
            MuduError::capture_backtrace(),
        ))
    }
}
