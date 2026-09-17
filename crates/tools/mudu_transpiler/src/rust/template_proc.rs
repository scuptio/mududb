//! Askama template data structures for the Rust procedure wrapper.

use askama::Template;

/// Wrapper template for a generated Mudu procedure module.
#[derive(Template)]
#[template(path = "rust/mudu_proc.rs.jinja", escape = "none")]
pub struct TemplateProc {
    /// Procedure metadata fed to the template.
    pub procedure: ProcedureInfo,
}

/// Metadata for one procedure argument.
pub struct ArgumentInfo {
    /// Argument name.
    pub arg_name: String,
    /// Argument Rust type string.
    pub arg_type: String,
    /// Zero-based argument index (excluding the OID).
    pub arg_index: usize,
    /// Whether the argument is binary (`Vec<u8>`).
    pub is_binary: bool,
}

/// Metadata for one procedure return value.
pub struct ReturnInfo {
    /// Return Rust type string.
    pub ret_type: String,
    /// Whether the return value is binary (`Vec<u8>`).
    pub is_binary: bool,
}

/// Full metadata for a procedure wrapper template.
pub struct ProcedureInfo {
    /// Generated module name.
    pub mod_name: String,
    /// Original procedure name.
    pub fn_name: String,
    /// Exported function name in WIT kebab-case.
    pub wit_fn_exported_name: String,
    /// WIT `async: true` annotation when async.
    pub wit_async_true: String,
    /// Exported P2 function name.
    pub fn_exported_name: String,
    /// Inner procedure function name.
    pub fn_inner_name: String,
    /// Guest struct name for the component interface.
    pub guest_struct_name: String,
    /// Argument descriptor constant name.
    pub fn_argv_desc: String,
    /// Result descriptor constant name.
    pub fn_result_desc: String,
    /// Procedure descriptor constant name.
    pub fn_proc_desc: String,
    /// Package/module name for descriptors.
    pub package_name: String,
    /// Procedure argument metadata.
    pub argument_list: Vec<ArgumentInfo>,
    /// Procedure return value metadata.
    pub return_tuple: Vec<ReturnInfo>,
    /// Number of return values.
    pub return_len: usize,
    /// `async` keyword when async.
    pub opt_async: String,
    /// `.await` suffix when async.
    pub opt_dot_await: String,
    /// `_async` suffix when async.
    pub opt_underline_async: String,
}
