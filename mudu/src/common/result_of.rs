use crate::common::error::ER;
use crate::common::result::RS;

pub fn rs_option<T>(opt: Option<T>, err_msg:&str) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(ER::NoneError(err_msg.to_string())),
    }
}

pub fn rs_of_opt<T, R: Fn() -> ER>(opt: Option<T>, fr: R) -> RS<T> {
    match opt {
        Some(t) => Ok(t),
        None => Err(fr()),
    }
}

pub fn rs_io<T>(r: Result<T, std::io::Error>) -> RS<T> {
    match r {
        Ok(t) => Ok(t),
        Err(e) => Err(ER::IOError(e.to_string())),
    }
}

pub fn std_io_error<T>(_r: std::io::Result<T>) -> RS<T> {
    match _r {
        Ok(t) => Ok(t),
        Err(e) => Err(ER::STDIOError(e.to_string())),
    }
}
