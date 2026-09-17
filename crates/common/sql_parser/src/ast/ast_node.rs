use std::any::Any;
use std::fmt::Debug;

/// Common marker trait for all SQL AST node types.
pub trait ASTNode: Any + Debug + Send + Sync {}
