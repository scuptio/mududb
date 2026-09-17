//! Canonical cross-language module paths shared by every mududb binding.
//!
//! The WIT interface `universal` (`crates/common/mudu_binding/wit/`) is the
//! single source of truth for the binding surface. Every language backend
//! maps it onto the same logical paths — `mududb.types` for the generated
//! Uni* value types and `mududb.codec` for the generated MSSP frame codec —
//! so import paths line up across AssemblyScript (`@mududb/mududb/types`),
//! C# (`mududb.types`), Python (`mududb.types`) and Rust (`mududb::types`).

/// Canonical module root shared by all language bindings.
pub const CANONICAL_ROOT: &str = "mududb";

/// WIT interface name of the universal binding surface.
pub const UNIVERSAL_INTERFACE: &str = "universal";

/// Canonical segment holding the generated Uni* value types.
pub const SEGMENT_TYPES: &str = "types";

/// Canonical segment holding the generated MSSP frame codec.
pub const SEGMENT_CODEC: &str = "codec";
