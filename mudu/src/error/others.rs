use crate::error::ec::EC;
use crate::error::err::MError;
use crate::m_error;

pub fn io_error(err: std::io::Error) -> MError {
    m_error!(EC::IOErr, "io error", err)
}
