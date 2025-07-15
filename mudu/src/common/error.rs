use crate::common::id::OID;
use crate::data_type::dt_fn_base::ErrConvert;
use crate::data_type::dt_fn_compare::ErrCompare;
use std::error::Error;
use std::fmt::{Display, Formatter};


#[derive(Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum ER {
    NoPrimaryKey = 1000,
    InternalError(String),
    NoSuchOID(OID),
    STDIOError(String),
    DecodeErr(String),
    EncodeErr(String),
    TupleErr,
    ValueConvertErr,
    CompareErr(ErrCompare),
    ConvertErr(ErrConvert),
    NoneError(String),
    NonErr,
    NotImplemented,
    NotYetImplemented(String),
    ParseError(String),
    NoSuchElement,
    ErrorType,
    IOError(String),
    ExistingSuchElement,
    SerdeError(String),
    FunctionNotImplemented,
    IndexOutOfRange,
    NoSuchTable(String),
    NoSuchColumn(String),
    MutexLockErr,
    PGWireError(String),
    AcceptError,
    BindError,
    ChSendError,
    PathError(String),
    ThreadSpawnError(String),
    BuildRuntimeError(String),
    ExistingSuchObject(String),
    TableInfoError(String),
    SQLParseError(String),
    ExecuteError(String),
    NoSuchTransaction(String),
    TxLockError(String),
    ExistingKey(String),
    NoExistingKey(String),
    ExistingTxInSession(String),
    JoinError(String),
    FatalError(String),
    MLParseError(String),
    WriteError(String),
    DBConnectError(String),
    MuduError(String),
}

impl Display for ER {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(format!("{:?}", self).as_str())
    }
}

impl Error for ER {}
