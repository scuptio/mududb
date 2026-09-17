//! Configuration for generated code traits and helpers.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
    /// Generate MSSP syscall frame/func codecs for WIT `func` items.
    pub with_func_codec: bool,
    /// Base name (PascalCase) for generated func-codec containers, e.g. the
    /// C# static class holding the per-syscall stubs. Derived from the input
    /// file stem; empty means the renderer picks its default.
    pub func_module_name: String,
    /// Type-kind registry: PascalCase type name → `enum`/`variant`/`record`,
    /// covering the definitions of ALL input WIT files. Needed for default
    /// value rendering: AssemblyScript enums are not constructable with
    /// `new X()` and C# variant (interface) types are not constructable
    /// either; populated by `gen_message` (directory mode) and `CodeGen`
    /// (same-file definitions).
    #[serde(default)]
    pub type_kinds: HashMap<String, String>,
}

impl CodegenCfg {
    /// Create a default configuration.
    pub fn new() -> CodegenCfg {
        Default::default()
    }
}
