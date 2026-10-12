use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::panic::Location;
use std::sync::Arc;

/// Error code of a type-system error.
#[derive(Debug, Clone, Copy)]
pub enum TyEC {
    /// Conversion between representations of a type failed.
    TypeConvertFailed,
    /// The destination buffer was too small for an encoded value.
    InsufficientSpace,
    /// An unexpected internal error of the type system.
    FatalInternalError,
    /// A type parameter string failed to parse.
    ParamParseError,
}

/// A type-system error: an error code, a message, and the source location
/// where the error was raised.
#[derive(Debug, Clone)]
pub struct TyErr {
    ec: TyEC,
    msg: String,
    #[allow(unused)]
    src: Option<Arc<dyn Error + Send + Sync + 'static>>,
    #[allow(unused)]
    loc: String,
}

impl Display for TyErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{0:?}", self))?;
        Ok(())
    }
}

impl Error for TyErr {}

impl TyErr {
    /// Creates an error with a code and message, recording the source
    /// location of the call.
    pub fn new(ec: TyEC, msg: String) -> Self {
        let loc = format!(
            "{}:{}",
            Location::caller().file(),
            Location::caller().line()
        );
        TyErr {
            ec,
            msg,
            src: None,
            loc,
        }
    }

    /// Creates an error with a code, message, and source error, recording
    /// the source location of the call.
    pub fn new_with_src<S: Error + Send + Sync + 'static>(ec: TyEC, msg: String, src: S) -> Self {
        let loc = format!(
            "{}:{}",
            Location::caller().file(),
            Location::caller().line()
        );
        TyErr {
            ec,
            msg,
            src: Some(Arc::new(src)),
            loc,
        }
    }

    /// Converts this error into a [`MuduError`], mapping the type error code
    /// to the corresponding [`ErrorCode`].
    pub fn to_m_err(&self) -> MuduError {
        match self.ec {
            TyEC::TypeConvertFailed | TyEC::ParamParseError => {
                mudu_error!(
                    ErrorCode::TypeConversionFailed,
                    self.msg.clone(),
                    self.clone()
                )
            }
            TyEC::InsufficientSpace => {
                mudu_error!(
                    ErrorCode::InsufficientBufferSpace,
                    self.msg.clone(),
                    self.clone()
                )
            }
            TyEC::FatalInternalError => {
                mudu_error!(ErrorCode::FatalInternal, self.msg.clone(), self.clone())
            }
        }
    }

    /// Returns the error message.
    pub fn msg(&self) -> String {
        self.msg.clone()
    }

    /// Returns the error code.
    pub fn ec(&self) -> TyEC {
        self.ec
    }
}
