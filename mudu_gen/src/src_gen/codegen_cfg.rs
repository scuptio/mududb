//! Configuration for generated code traits and helpers.

use serde::{Deserialize, Serialize};

/// Configuration controlling which extra traits/methods are generated.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodegenCfg {
    /// Generate inner helper functions.
    pub impl_inner_func: bool,
    /// Generate `Serialize`/`Deserialize` impls.
    pub impl_serialize: bool,
    /// Generate a `Default` impl.
    pub impl_default: bool,
    /// Generate a `Display` impl.
    pub impl_display: bool,
    /// Generate a `FromStr` impl.
    pub impl_from_str: bool,
    /// Generate an `Eq` impl.
    pub impl_eq: bool,
    /// Generate a `Hash` impl.
    pub impl_hash: bool,
}

impl CodegenCfg {
    /// Create a default configuration.
    pub fn new() -> CodegenCfg {
        Default::default()
    }
}
