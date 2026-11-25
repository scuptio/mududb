use crate::ast::ast_node::ASTNode;

/// `ACK <name> [FOR <consumer>] TO <seq>` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtAck {
    name: String,
    seq: u64,
    consumer: Option<String>,
}

impl StmtAck {
    /// Create a new `ACK` statement.
    pub fn new(name: String, seq: u64) -> Self {
        Self {
            name,
            seq,
            consumer: None,
        }
    }

    /// Return the queue name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the acknowledged sequence number.
    pub fn seq(&self) -> u64 {
        self.seq
    }

    /// Set the named consumer cursor (`FOR <consumer>`) this ack advances;
    /// absent means the default competing cursor.
    pub fn set_consumer(&mut self, consumer: String) {
        self.consumer = Some(consumer);
    }

    /// Return the named consumer cursor, if a `FOR` clause was specified.
    pub fn consumer(&self) -> Option<&str> {
        self.consumer.as_deref()
    }
}

impl ASTNode for StmtAck {}
