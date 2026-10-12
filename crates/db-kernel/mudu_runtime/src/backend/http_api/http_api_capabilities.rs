/// Feature switches selecting which route groups the management HTTP API
/// serves.
#[derive(Clone, Copy, Debug)]
pub struct HttpApiCapabilities {
    /// Enables the procedure invoke endpoint.
    pub enable_invoke: bool,
    /// Enables the application uninstall endpoint.
    pub enable_uninstall: bool,
    /// Enables the replication admin endpoints (membership/promote/follow).
    pub enable_repl_admin: bool,
}

impl HttpApiCapabilities {
    /// Legacy backend capabilities: procedure invoke only.
    pub const LEGACY: Self = Self {
        enable_invoke: true,
        enable_uninstall: false,
        enable_repl_admin: false,
    };

    /// Kernel backend capabilities: invoke, uninstall, and replication admin.
    pub const IOURING: Self = Self {
        enable_invoke: true,
        enable_uninstall: true,
        enable_repl_admin: true,
    };
}
