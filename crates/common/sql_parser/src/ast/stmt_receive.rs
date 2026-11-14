use crate::ast::ast_node::ASTNode;

/// Wait behavior of a `RECEIVE` statement.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReceiveWait {
    /// `WAIT <ms>`: wait at most the given number of milliseconds.
    Timeout(u64),
    /// `WAIT FOREVER`: wait without a timeout.
    Forever,
}

/// `RECEIVE [n] FROM <name> [FOR <consumer>] [WAIT <ms> | WAIT FOREVER]`
/// statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtReceive {
    name: String,
    count: Option<u64>,
    wait: Option<ReceiveWait>,
    consumer: Option<String>,
}

impl StmtReceive {
    /// Create a new `RECEIVE` statement.
    pub fn new(name: String, count: Option<u64>, wait: Option<ReceiveWait>) -> Self {
        Self {
            name,
            count,
            wait,
            consumer: None,
        }
    }

    /// Return the queue name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the maximum number of messages to receive, if specified.
    pub fn count(&self) -> Option<u64> {
        self.count
    }

    /// Return the wait behavior, if a `WAIT` clause was specified.
    pub fn wait(&self) -> Option<ReceiveWait> {
        self.wait
    }

    /// Set the named consumer cursor (`FOR <consumer>`) this receive reads
    /// from; absent means the default competing cursor.
    pub fn set_consumer(&mut self, consumer: String) {
        self.consumer = Some(consumer);
    }

    /// Return the named consumer cursor, if a `FOR` clause was specified.
    pub fn consumer(&self) -> Option<&str> {
        self.consumer.as_deref()
    }
}

impl ASTNode for StmtReceive {}
