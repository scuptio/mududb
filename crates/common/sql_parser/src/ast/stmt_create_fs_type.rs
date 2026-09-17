use crate::ast::ast_node::ASTNode;

/// Kind of a filesystem type created by `CREATE TYPE FILESYSTEM`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FsTypeKind {
    /// Filesystem file type.
    File,
    /// Filesystem directory type.
    Directory,
}

/// `CREATE TYPE FILESYSTEM FILE|DIRECTORY <name>` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtCreateFsType {
    name: String,
    kind: FsTypeKind,
}

impl StmtCreateFsType {
    /// Create a new `CREATE TYPE FILESYSTEM` statement.
    pub fn new(name: String, kind: FsTypeKind) -> Self {
        Self { name, kind }
    }

    /// Return the filesystem type name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the filesystem type kind.
    pub fn kind(&self) -> FsTypeKind {
        self.kind
    }
}

impl ASTNode for StmtCreateFsType {}
