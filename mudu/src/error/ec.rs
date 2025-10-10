use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};


/// Error code
#[derive(
    Debug, Clone, PartialEq, Eq,
    Serialize, Deserialize,
    IntoPrimitive, TryFromPrimitive
)]
#[repr(u32)]
pub enum EC {
    Ok = 0,
    InternalErr = 1000,
    DecodeErr,
    EncodeErr,
    TupleErr,
    CompareErr,
    ConvertErr,
    NoneErr,
    NotImplemented,
    ParseErr,
    NoSuchElement,
    TypeErr,
    IOErr,
    ExistingSuchElement,
    FunctionNotImplemented,
    IndexOutOfRange,
    MLParseError,
    FmtWriteErr,
    MuduError,
    WASMMemoryAccessError,
    InsufficientBufferSpace,
    MutexError,
    DBInternalError,
    TxErr,
    NetErr,
    SyncErr,
    /// fatal error possible be a bug
    FatalError,
    ThreadErr,
    TokioErr,
}

impl Display for EC {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl EC {
    pub fn message(&self) -> &'static str {
        match self {
            EC::Ok => "OK",
            EC::InternalErr => "Internal error",
            EC::DecodeErr => "Decode error",
            EC::EncodeErr => "Encode error",
            EC::TupleErr => "Tuple error",
            EC::CompareErr => "Compare error",
            EC::ConvertErr => "Convert error",
            EC::NoneErr => "None Error",
            EC::NotImplemented => "Not Implemented",
            EC::ParseErr => "Parse error",
            EC::NoSuchElement => "No such element error",
            EC::TypeErr => "Type error",
            EC::IOErr => "IO error",
            EC::ExistingSuchElement => "Existing such element",
            EC::FunctionNotImplemented => "Function not implemented for this type",
            EC::IndexOutOfRange => "Index out of range",
            EC::MLParseError => "ML parse error",
            EC::FmtWriteErr => "Format write error",
            EC::MuduError => "MUDU error",
            EC::WASMMemoryAccessError => "WASM memory access error",
            EC::InsufficientBufferSpace => "Insufficient buffer space",
            EC::MutexError => "Mutex error",
            EC::DBInternalError => "DB open error",
            EC::TxErr => "Transaction error",
            EC::NetErr => "Net error",
            EC::SyncErr => "Synchronized error",
            EC::FatalError => "Fatal error",
            EC::ThreadErr => "Thread error",
            EC::TokioErr => "Tokio error",
        }
    }
}
impl Error for EC {}
