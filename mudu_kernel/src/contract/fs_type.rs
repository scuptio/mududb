use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

/// Kind of a filesystem type registered in the catalog.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsTypeKind {
    /// Filesystem file type.
    File = 1,
    /// Filesystem directory type.
    Directory = 2,
}

impl FsTypeKind {
    /// Return the stable `u32` code of this kind.
    pub fn as_u32(&self) -> u32 {
        match self {
            FsTypeKind::File => 1,
            FsTypeKind::Directory => 2,
        }
    }

    /// Convert a `u32` code back into a filesystem type kind.
    pub fn from_u32(value: u32) -> RS<Self> {
        match value {
            1 => Ok(FsTypeKind::File),
            2 => Ok(FsTypeKind::Directory),
            _ => Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("invalid filesystem type kind {}", value)
            )),
        }
    }
}

/// Descriptor of a filesystem type registered in the catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsTypeDesc {
    name: String,
    fs_id: u64,
    kind: FsTypeKind,
}

impl FsTypeDesc {
    /// Create a new filesystem type descriptor.
    pub fn new(name: String, fs_id: u64, kind: FsTypeKind) -> Self {
        Self { name, fs_id, kind }
    }

    /// Return the filesystem type name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the allocated filesystem id.
    pub fn fs_id(&self) -> u64 {
        self.fs_id
    }

    /// Return the filesystem type kind.
    pub fn kind(&self) -> FsTypeKind {
        self.kind
    }
}

/// Binding recorded on a table column whose declared type is a registered
/// filesystem type name. The column is physically stored as `U128`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsColumnBinding {
    fs_id: u64,
    kind: FsTypeKind,
}

impl FsColumnBinding {
    /// Create a new filesystem column binding.
    pub fn new(fs_id: u64, kind: FsTypeKind) -> Self {
        Self { fs_id, kind }
    }

    /// Return the bound filesystem type id.
    pub fn fs_id(&self) -> u64 {
        self.fs_id
    }

    /// Return the bound filesystem type kind.
    pub fn kind(&self) -> FsTypeKind {
        self.kind
    }
}
