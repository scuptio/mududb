#![allow(missing_docs)]

#[derive(Clone, Copy, Debug)]
pub struct HttpApiCapabilities {
    pub enable_invoke: bool,
    pub enable_uninstall: bool,
}

impl HttpApiCapabilities {
    pub const LEGACY: Self = Self {
        enable_invoke: true,
        enable_uninstall: false,
    };

    pub const IOURING: Self = Self {
        enable_invoke: true,
        enable_uninstall: true,
    };
}
