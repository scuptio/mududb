//! Name prefixes used for procedures exported from or consumed by guest modules.

/// Prefix of the P2 (protocol version 2) procedure symbol names.
pub const MUDU_PROC_P2_PREFIX: &str = "mp2_";

/// Prefix of the exported P2 procedure symbol names.
pub const MUDU_PROC_P2_EXPORTED_PREFIX: &str = "mp2e_";

/// Prefix of internal procedure symbol names.
pub const MUDU_PROC_INNER_PREFIX: &str = "mudu_inner_";
/// Prefix of internal P2 procedure symbol names.
pub const MUDU_PROC_INNER_PREFIX_P2: &str = "mudu_inner_p2_";
/// Prefix of the symbol carrying a procedure's argument descriptor.
pub const MUDU_PROC_ARGV_DESC_PREFIX: &str = "mudu_argv_desc_";
/// Prefix of the symbol carrying a procedure's result descriptor.
pub const MUDU_PROC_RESULT_DESC_PREFIX: &str = "mudu_result_desc_";
/// Prefix of the symbol carrying a procedure's full descriptor.
pub const MUDU_PROC_PROC_DESC_PREFIX: &str = "mudu_proc_desc_";
/// Prefix marking guest-side procedure names.
pub const MUDU_PROC_PREFIX_GUEST: &str = "Guest";
/// Prefix of module-level symbol names.
pub const MUDU_PROC_PREFIX_MOD: &str = "mod_";
