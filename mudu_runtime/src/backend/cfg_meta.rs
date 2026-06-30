//! Configuration metadata for `MuduDBCfg`.
//!
//! This module classifies every configuration field by how it can be changed
//! after the server has been started for the first time.

/// How a configuration value may be changed during the lifetime of a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigMutability {
    /// Set once when the database directory is created.
    ///
    /// Changing this value for an existing database requires a migration tool
    /// that rewrites persisted data (for example `page_size`).
    Persistent,
    /// May be changed in the configuration file, but only takes effect after
    /// the server process is restarted.
    RestartRequired,
    /// May be changed while the server is running, usually through an admin
    /// command or a future hot-reload mechanism.
    Runtime,
}

impl ConfigMutability {
    /// Returns a short human-readable label for this mutability class.
    pub fn as_str(self) -> &'static str {
        match self {
            ConfigMutability::Persistent => "persistent",
            ConfigMutability::RestartRequired => "restart-required",
            ConfigMutability::Runtime => "runtime",
        }
    }
}
