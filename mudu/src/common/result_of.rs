use crate::common::result::RS;
use crate::error::ec::EC;
use crate::error::err::MError;
use crate::m_error;

pub fn rs_option<T>(opt: Option<T>, err_msg: &str) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(m_error!(EC::NoneErr, err_msg)),
    }
}

pub fn rs_of_opt<T, R: Fn() -> MError>(opt: Option<T>, fr: R) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(fr()),
    }
}

pub fn rs_io<T>(r: Result<T, std::io::Error>) -> RS<T> {
    match r {
        Ok(t) => Ok(t),
        Err(e) => Err(m_error!(EC::IOErr, "io error", e)),
    }
}

pub fn std_io_error<T>(_r: std::io::Result<T>) -> RS<T> {
    match _r {
        Ok(t) => Ok(t),
        Err(e) => Err(m_error!(EC::IOErr, "io error", e)),
    }
}
