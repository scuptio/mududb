//! C MessagePack codec snippet generation.
//!
//! Produces explicit `mp_writer`/`mp_reader` statement lists (the runtime in
//! `crates/sdk/bindings/c/mududb/codec/mpack.h` exposes only explicit
//! per-type functions and C has no generics, so templates cannot use a
//! generic `write<T>`/`read<T>` style). Shared by the record/variant/enum/
//! func C templates; mirrors the AssemblyScript blueprint
//! (`lang_impl/assemblyscript/as_codec.rs`) statement for statement.
//!
//! The pinned dual semantics of `list<u8>` are controlled by
//! [`CCodecStyle::bytes_as_array`]: inside record/variant definitions a
//! `list<u8>` value is a plain MessagePack ARRAY of `u8` (matching the Rust
//! host's `Vec<u8>` serde output), while at func signature level it is a
//! MessagePack **bin** blob.

use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};

mod ctype;
mod decode;
mod encode;
mod holder;
mod map_decode;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests;

pub use ctype::c_type;
pub use decode::c_decode_stmts;
pub use encode::c_encode_stmts;
pub use holder::CollectingCNamer;
pub use map_decode::{CMapEntry, c_map_decode_loop};

/// Codec generation style selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CCodecStyle {
    /// Whether `list<u8>`/`blob` values encode as plain MessagePack arrays of
    /// `u8` (record/variant context) instead of MessagePack bin (func
    /// context).
    pub bytes_as_array: bool,
}

/// Record/variant definition context: `list<u8>` is a MessagePack array.
pub const C_STYLE_RECORD: CCodecStyle = CCodecStyle {
    bytes_as_array: true,
};

/// Func signature context: `list<u8>` is a MessagePack bin blob.
pub const C_STYLE_FUNC: CCodecStyle = CCodecStyle {
    bytes_as_array: false,
};

/// The C type name of a user-defined WIT type (`uni-oid` -> `uni_oid`).
pub fn c_type_name(wit_name: &str) -> String {
    to_snake_case(wit_name)
}

/// The C constant prefix of a type name (`uni-oid` -> `UNI_OID`).
pub fn c_const_name(wit_name: &str) -> String {
    to_snake_case_upper(wit_name)
}

/// C and C++ keywords plus the `stdbool.h`/`stddef.h` macros (the generated
/// headers must also compile as C++).
const C_CPP_KEYWORDS: &[&str] = &[
    // C keywords
    "auto",
    "break",
    "case",
    "char",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "extern",
    "float",
    "for",
    "goto",
    "if",
    "inline",
    "int",
    "long",
    "register",
    "restrict",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "unsigned",
    "void",
    "volatile",
    "while",
    "_Alignas",
    "_Alignof",
    "_Atomic",
    "_Bool",
    "_Complex",
    "_Generic",
    "_Imaginary",
    "_Noreturn",
    "_Static_assert",
    "_Thread_local",
    // stdbool.h / stddef.h macros
    "bool",
    "true",
    "false",
    "NULL",
    // C++ keywords (the headers must also compile as C++)
    "alignas",
    "alignof",
    "and",
    "and_eq",
    "asm",
    "bitand",
    "bitor",
    "catch",
    "char16_t",
    "char32_t",
    "class",
    "compl",
    "constexpr",
    "const_cast",
    "decltype",
    "delete",
    "dynamic_cast",
    "explicit",
    "export",
    "friend",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "not",
    "not_eq",
    "nullptr",
    "operator",
    "or",
    "or_eq",
    "private",
    "protected",
    "public",
    "reinterpret_cast",
    "static_assert",
    "static_cast",
    "template",
    "this",
    "thread_local",
    "throw",
    "try",
    "typeid",
    "typename",
    "using",
    "virtual",
    "wchar_t",
    "xor",
    "xor_eq",
    "override",
    "final",
];

/// Sanitize a snake-case identifier that collides with a C or C++ keyword
/// (or a `stdbool.h`/`stddef.h` macro) by appending an underscore.
pub fn c_ident(name: &str) -> String {
    if C_CPP_KEYWORDS.contains(&name) {
        format!("{}_", name)
    } else {
        name.to_string()
    }
}

pub(super) fn pad(indent: usize) -> String {
    " ".repeat(indent)
}

/// Join snippet lines into a single template-ready string.
pub fn c_join(lines: &[String]) -> String {
    lines.join("\n")
}
