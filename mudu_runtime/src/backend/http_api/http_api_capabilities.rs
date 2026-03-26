#[derive(Clone, Copy)]
pub struct HttpApiCapabilities {
    pub enable_invoke: bool,
    pub enable_uninstall: bool,
}

impl HttpApiCapabilities {
    pub const LEGACY: Self = Self {
        enable_invoke: true,
        enable_uninstall: false,
    };

    #[cfg(target_os = "linux")]
    pub const IOURING: Self = Self {
        enable_invoke: true,
        enable_uninstall: true,
    };
}
