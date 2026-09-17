use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::panic::Location;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub enum TyEC {
    TypeConvertFailed,
    InsufficientSpace,
    FatalInternalError,
    ParamParseError,
}

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

    pub fn msg(&self) -> String {
        self.msg.clone()
    }

    pub fn ec(&self) -> TyEC {
        self.ec
    }
}
