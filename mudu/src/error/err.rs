use crate::error::ec::EC;
use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Debug)]
pub struct MError {
    ec: EC,
    msg: String,
    src: Option<Box<dyn Error>>,
}

impl MError {
    pub fn new_with_ec(ec: EC) -> Self {
        Self::new(ec, String::default(), None)
    }

    pub fn new_with_ec_msg<S: AsRef<str>>(ec: EC, s: S) -> Self {
        Self::new(ec, s, None)
    }

    pub fn new_with_ec_msg_src<S: AsRef<str>, E: Into<Box<dyn Error + 'static>>, >(
        ec: EC,
        s: S,
        src: E,
    ) -> MError {
        Self::new(ec, s, Some(src.into()))
    }

    pub fn new<S: AsRef<str>>(ec: EC, s: S, src: Option<Box<dyn Error>>) -> Self {
        let msg = if s.as_ref().is_empty() {
            ec.message().to_string()
        } else {
            s.as_ref().to_string()
        };
        Self {
            ec,
            msg,
            src,
        }
    }

    pub fn ec(&self) -> EC {
        self.ec.clone()
    }

    pub fn message(&self) -> &str {
        &self.msg
    }
}


impl Display for MError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl Error for MError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.src.as_ref().map(|e| e.as_ref())
    }
}


#[macro_export]
macro_rules! m_error {
    ($ec:expr) => {
        $crate::error::err::MError::new_with_ec($ec)
    };
    ($ec:expr, $msg:expr) => {
        $crate::error::err::MError::new_with_ec_msg($ec, $msg)
    };
    ($ec:expr, $msg:expr, $src:expr) => {
        $crate::error::err::MError::new_with_ec_msg_src($ec, $msg, $src)
    };
}

impl Eq for MError {}

impl PartialEq for MError {
    fn eq(&self, other: &Self) -> bool {
        self.ec == other.ec && self.msg == other.msg
    }
}

impl Default for MError {
    fn default() -> Self {
        Self::new_with_ec(EC::Ok)
    }
}

unsafe impl Sync for MError {}

unsafe impl Send for MError {}
