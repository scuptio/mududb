use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::io::ErrorKind;
use strum::EnumMessage;

/// Classification of an error's impact / handling strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Caused by invalid user input; should be reported to the caller.
    User,
    /// Indicates a bug or invariant violation inside the system.
    Internal,
    /// May succeed on retry (network blip, lock contention, etc.).
    Transient,
}

/// Stable error codes exposed by MuduDB.
///
/// I/O errors use their canonical Linux/POSIX errno value where an
/// [`ErrorKind`] has a direct errno equivalent. I/O kinds without one use the
/// reserved range `1000..=1999`. Application errors start at `50000`.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Copy,
    Serialize,
    Deserialize,
    IntoPrimitive,
    TryFromPrimitive,
    strum::EnumIter,
    strum::EnumMessage,
)]
#[repr(u32)]
pub enum ErrorCode {
    // ErrorKind values backed by a canonical Linux/POSIX errno.
    #[strum(message = "Permission denied")]
    PermissionDenied = 13, // EACCES
    #[strum(message = "I/O entity not found")]
    NotFound = 2, // ENOENT
    #[strum(message = "Operation interrupted")]
    Interrupted = 4, // EINTR
    #[strum(message = "Argument list too long")]
    ArgumentListTooLong = 7, // E2BIG
    #[strum(message = "Bad file descriptor")]
    BadFileDescriptor = 9, // EBADF (raw errno only)
    #[strum(message = "Operation would block")]
    WouldBlock = 11, // EAGAIN
    #[strum(message = "Out of memory")]
    OutOfMemory = 12, // ENOMEM
    #[strum(message = "Resource busy")]
    ResourceBusy = 16, // EBUSY
    #[strum(message = "I/O entity already exists")]
    AlreadyExists = 17, // EEXIST
    #[strum(message = "Operation crosses devices")]
    CrossesDevices = 18, // EXDEV
    #[strum(message = "Not a directory")]
    NotADirectory = 20, // ENOTDIR
    #[strum(message = "Is a directory")]
    IsADirectory = 21, // EISDIR
    #[strum(message = "Invalid I/O input")]
    InvalidInput = 22, // EINVAL
    #[strum(message = "Executable file busy")]
    ExecutableFileBusy = 26, // ETXTBSY
    #[strum(message = "File too large")]
    FileTooLarge = 27, // EFBIG
    #[strum(message = "Storage full")]
    StorageFull = 28, // ENOSPC
    #[strum(message = "Resource is not seekable")]
    NotSeekable = 29, // ESPIPE
    #[strum(message = "Read-only filesystem")]
    ReadOnlyFilesystem = 30, // EROFS
    #[strum(message = "Too many links")]
    TooManyLinks = 31, // EMLINK
    #[strum(message = "Broken pipe")]
    BrokenPipe = 32, // EPIPE
    #[strum(message = "Deadlock")]
    Deadlock = 35, // EDEADLK
    #[strum(message = "Invalid filename")]
    InvalidFilename = 36, // ENAMETOOLONG
    #[strum(message = "Directory not empty")]
    DirectoryNotEmpty = 39, // ENOTEMPTY
    #[strum(message = "Filesystem loop")]
    FilesystemLoop = 40, // ELOOP
    #[strum(message = "Unsupported I/O operation")]
    Unsupported = 95, // EOPNOTSUPP
    #[strum(message = "Address already in use")]
    AddrInUse = 98, // EADDRINUSE
    #[strum(message = "Address not available")]
    AddrNotAvailable = 99, // EADDRNOTAVAIL
    #[strum(message = "Network down")]
    NetworkDown = 100, // ENETDOWN
    #[strum(message = "Network unreachable")]
    NetworkUnreachable = 101, // ENETUNREACH
    #[strum(message = "Connection aborted")]
    ConnectionAborted = 103, // ECONNABORTED
    #[strum(message = "Connection reset")]
    ConnectionReset = 104, // ECONNRESET
    #[strum(message = "Not connected")]
    NotConnected = 107, // ENOTCONN
    #[strum(message = "Operation timed out")]
    TimedOut = 110, // ETIMEDOUT
    #[strum(message = "Connection refused")]
    ConnectionRefused = 111, // ECONNREFUSED
    #[strum(message = "Host unreachable")]
    HostUnreachable = 113, // EHOSTUNREACH
    #[strum(message = "Operation in progress")]
    InProgress = 115, // EINPROGRESS
    #[strum(message = "Stale network file handle")]
    StaleNetworkFileHandle = 116, // ESTALE
    #[strum(message = "Quota exceeded")]
    QuotaExceeded = 122, // EDQUOT

    // ErrorKind values without a unique errno.
    #[strum(message = "Invalid I/O data")]
    InvalidData = 1000,
    #[strum(message = "Write returned zero bytes")]
    WriteZero = 1001,
    #[strum(message = "Unexpected end of file")]
    UnexpectedEof = 1002,
    #[strum(message = "Other I/O error")]
    Other = 1003,
    #[strum(message = "Uncategorized I/O error")]
    Uncategorized = 1004,

    // MuduDB/application errors.
    #[strum(message = "Internal error")]
    Internal = 50000,
    #[strum(message = "Decode error")]
    Decode = 50001,
    #[strum(message = "Encode error")]
    Encode = 50002,
    #[strum(message = "Invalid tuple")]
    InvalidTuple = 50003,
    #[strum(message = "Comparison failed")]
    ComparisonFailed = 50004,
    #[strum(message = "Type conversion failed")]
    TypeConversionFailed = 50005,
    #[strum(message = "Invalid state")]
    InvalidState = 50006,
    #[strum(message = "Not implemented")]
    NotImplemented = 50007,
    #[strum(message = "Parse error")]
    Parse = 50008,
    #[strum(message = "Entity not found")]
    EntityNotFound = 50009,
    #[strum(message = "Invalid type")]
    InvalidType = 50010,
    #[strum(message = "I/O error")]
    Io = 50011,
    #[strum(message = "Entity already exists")]
    EntityAlreadyExists = 50012,
    #[strum(message = "Unsupported operation")]
    UnsupportedOperation = 50013,
    #[strum(message = "Index out of range")]
    IndexOutOfRange = 50014,
    #[strum(message = "ML parse error")]
    MlParse = 50015,
    #[strum(message = "Format write error")]
    FmtWrite = 50016,
    #[strum(message = "Domain violation")]
    DomainViolation = 50017,
    #[strum(message = "WASM memory access error")]
    WasmMemoryAccess = 50018,
    #[strum(message = "Insufficient buffer space")]
    InsufficientBufferSpace = 50019,
    #[strum(message = "Mutex error")]
    Mutex = 50020,
    #[strum(message = "Database error")]
    Database = 50021,
    #[strum(message = "Transaction error")]
    Transaction = 50022,
    #[strum(message = "Network error")]
    Network = 50023,
    #[strum(message = "Synchronization error")]
    Synchronization = 50024,
    /// Fatal internal error that may indicate a bug.
    #[strum(message = "Fatal internal error")]
    FatalInternal = 50025,
    #[strum(message = "Thread error")]
    Thread = 50026,
    #[strum(message = "Tokio error")]
    Tokio = 50027,
    #[strum(message = "External error")]
    External = 50028,
    #[strum(message = "Invalid argument")]
    InvalidArgument = 50029,
    #[strum(message = "Invalid UTF-8")]
    InvalidUtf8 = 50030,
    #[strum(message = "Path contains interior NUL")]
    PathContainsNul = 50031,
    #[strum(message = "Network partition")]
    NetworkPartition = 50032,
    #[strum(message = "Listen backlog full")]
    ListenBacklogFull = 50033,
    #[strum(message = "Channel closed")]
    ChannelClosed = 50034,
    #[strum(message = "Storage error")]
    Storage = 50035,
    #[strum(message = "Hash failed")]
    HashFailed = 50036,
    #[strum(message = "Unsupported format version")]
    UnsupportedFormatVersion = 50037,
    #[strum(message = "Corrupted data")]
    CorruptedData = 50038,
    #[strum(message = "Incompatible protocol version")]
    IncompatibleProtocolVersion = 50039,
}

impl Display for ErrorCode {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message())
    }
}

impl ErrorCode {
    /// Returns the numeric protocol representation.
    pub fn to_u32(&self) -> u32 {
        (*self).into()
    }

    /// Decodes a defined protocol error code.
    pub fn from_u32(ec: u32) -> Option<ErrorCode> {
        ErrorCode::try_from_primitive(ec).ok()
    }

    /// Returns an iterator over all defined error codes.
    pub fn iter() -> impl Iterator<Item = ErrorCode> {
        <Self as strum::IntoEnumIterator>::iter()
    }

    /// Converts a raw POSIX errno through `std::io::ErrorKind`.
    /// Unknown errno values map to [`ErrorCode::Uncategorized`].
    pub fn from_raw_os_error(errno: i32) -> ErrorCode {
        ErrorCode::from(&std::io::Error::from_raw_os_error(errno))
    }

    /// Returns the corresponding [`ErrorKind`] for I/O error codes.
    pub fn error_kind(self) -> Option<ErrorKind> {
        ErrorKind::try_from(self).ok()
    }

    /// Returns a default human-readable description.
    pub fn message(&self) -> &'static str {
        self.get_message().unwrap_or("Unknown error")
    }

    /// Returns the severity classification for this error code.
    pub fn severity(self) -> Severity {
        match self {
            ErrorCode::PermissionDenied
            | ErrorCode::NotFound
            | ErrorCode::InvalidInput
            | ErrorCode::InvalidData
            | ErrorCode::InvalidFilename
            | ErrorCode::ArgumentListTooLong
            | ErrorCode::AlreadyExists
            | ErrorCode::NotADirectory
            | ErrorCode::IsADirectory
            | ErrorCode::DirectoryNotEmpty
            | ErrorCode::InvalidTuple
            | ErrorCode::InvalidType
            | ErrorCode::InvalidArgument
            | ErrorCode::InvalidUtf8
            | ErrorCode::PathContainsNul
            | ErrorCode::DomainViolation
            | ErrorCode::IndexOutOfRange
            | ErrorCode::UnsupportedOperation
            | ErrorCode::EntityNotFound
            | ErrorCode::EntityAlreadyExists => Severity::User,

            ErrorCode::TimedOut
            | ErrorCode::WouldBlock
            | ErrorCode::Interrupted
            | ErrorCode::InProgress
            | ErrorCode::ConnectionRefused
            | ErrorCode::ConnectionReset
            | ErrorCode::ConnectionAborted
            | ErrorCode::NotConnected
            | ErrorCode::NetworkDown
            | ErrorCode::NetworkUnreachable
            | ErrorCode::HostUnreachable
            | ErrorCode::AddrInUse
            | ErrorCode::AddrNotAvailable
            | ErrorCode::BrokenPipe
            | ErrorCode::NetworkPartition
            | ErrorCode::ListenBacklogFull
            | ErrorCode::ChannelClosed
            | ErrorCode::ResourceBusy
            | ErrorCode::Deadlock
            | ErrorCode::Mutex
            | ErrorCode::Synchronization => Severity::Transient,

            _ => Severity::Internal,
        }
    }
}

impl From<ErrorKind> for ErrorCode {
    fn from(kind: ErrorKind) -> Self {
        match kind {
            ErrorKind::NotFound => ErrorCode::NotFound,
            ErrorKind::PermissionDenied => ErrorCode::PermissionDenied,
            ErrorKind::ConnectionRefused => ErrorCode::ConnectionRefused,
            ErrorKind::ConnectionReset => ErrorCode::ConnectionReset,
            ErrorKind::ConnectionAborted => ErrorCode::ConnectionAborted,
            ErrorKind::NotConnected => ErrorCode::NotConnected,
            ErrorKind::AddrInUse => ErrorCode::AddrInUse,
            ErrorKind::AddrNotAvailable => ErrorCode::AddrNotAvailable,
            ErrorKind::BrokenPipe => ErrorCode::BrokenPipe,
            ErrorKind::AlreadyExists => ErrorCode::AlreadyExists,
            ErrorKind::WouldBlock => ErrorCode::WouldBlock,
            ErrorKind::InvalidInput => ErrorCode::InvalidInput,
            ErrorKind::InvalidData => ErrorCode::InvalidData,
            ErrorKind::TimedOut => ErrorCode::TimedOut,
            ErrorKind::WriteZero => ErrorCode::WriteZero,
            ErrorKind::Interrupted => ErrorCode::Interrupted,
            ErrorKind::Unsupported => ErrorCode::Unsupported,
            ErrorKind::UnexpectedEof => ErrorCode::UnexpectedEof,
            ErrorKind::OutOfMemory => ErrorCode::OutOfMemory,
            ErrorKind::Other => ErrorCode::Other,
            _ => ErrorCode::Uncategorized,
        }
    }
}

impl TryFrom<ErrorCode> for ErrorKind {
    type Error = ();

    fn try_from(ec: ErrorCode) -> Result<Self, Self::Error> {
        match ec {
            ErrorCode::NotFound => Ok(ErrorKind::NotFound),
            ErrorCode::PermissionDenied => Ok(ErrorKind::PermissionDenied),
            ErrorCode::ConnectionRefused => Ok(ErrorKind::ConnectionRefused),
            ErrorCode::ConnectionReset => Ok(ErrorKind::ConnectionReset),
            ErrorCode::ConnectionAborted => Ok(ErrorKind::ConnectionAborted),
            ErrorCode::NotConnected => Ok(ErrorKind::NotConnected),
            ErrorCode::AddrInUse => Ok(ErrorKind::AddrInUse),
            ErrorCode::AddrNotAvailable => Ok(ErrorKind::AddrNotAvailable),
            ErrorCode::BrokenPipe => Ok(ErrorKind::BrokenPipe),
            ErrorCode::AlreadyExists => Ok(ErrorKind::AlreadyExists),
            ErrorCode::WouldBlock => Ok(ErrorKind::WouldBlock),
            ErrorCode::InvalidInput => Ok(ErrorKind::InvalidInput),
            ErrorCode::InvalidData => Ok(ErrorKind::InvalidData),
            ErrorCode::TimedOut => Ok(ErrorKind::TimedOut),
            ErrorCode::WriteZero => Ok(ErrorKind::WriteZero),
            ErrorCode::Interrupted => Ok(ErrorKind::Interrupted),
            ErrorCode::Unsupported => Ok(ErrorKind::Unsupported),
            ErrorCode::UnexpectedEof => Ok(ErrorKind::UnexpectedEof),
            ErrorCode::OutOfMemory => Ok(ErrorKind::OutOfMemory),
            ErrorCode::Other => Ok(ErrorKind::Other),
            _ => Err(()),
        }
    }
}

impl From<&std::io::Error> for ErrorCode {
    fn from(error: &std::io::Error) -> Self {
        // ErrorCode's OS values intentionally use canonical Linux errno values.
        // Prefer the raw value on Linux so stable Rust can preserve detailed
        // errors whose ErrorKind variants are still unstable.
        #[cfg(target_os = "linux")]
        if let Some(errno) = error.raw_os_error()
            && let Ok(raw) = u32::try_from(errno)
            && let Some(code) = ErrorCode::from_u32(raw)
            && code.to_u32() < 1000
        {
            return code;
        }
        ErrorCode::from(error.kind())
    }
}

impl From<std::io::Error> for ErrorCode {
    fn from(error: std::io::Error) -> Self {
        ErrorCode::from(&error)
    }
}

impl Error for ErrorCode {}
