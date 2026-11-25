use crate::ast::ast_node::ASTNode;

/// `CREATE CACHE <name> LIKE <table> [TTL <n>]` statement AST node.
///
/// A volatile, non-replicated cache declaration that mirrors an existing
/// table's shape. A cache never defines its own columns; it always references
/// a table via `LIKE`.
#[derive(Clone, Debug)]
pub struct StmtCreateCache {
    name: String,
    like: String,
    ttl_secs: Option<u64>,
}

impl StmtCreateCache {
    /// Creates a `CREATE CACHE` statement.
    pub fn new(name: String, like: String, ttl_secs: Option<u64>) -> Self {
        Self {
            name,
            like,
            ttl_secs,
        }
    }

    /// The cache name.
    pub fn name(&self) -> &String {
        &self.name
    }

    /// The mirrored table name.
    pub fn like(&self) -> &String {
        &self.like
    }

    /// The optional TTL in seconds.
    pub fn ttl_secs(&self) -> Option<u64> {
        self.ttl_secs
    }
}

impl ASTNode for StmtCreateCache {}
