//! Non-scalar type shapes used by language dispatch functions.

/// A non-scalar WIT type parameterized by its rendered inner type(s).
///
/// This module defines the shape of composite WIT types after their inner
/// components have already been rendered to language-specific names.
pub enum NonScalarType {
    /// Array/list type.
    Array(String),
    /// Optional type.
    Option(String),
    /// Boxed type.
    Box(String),
    /// Tuple type.
    Tuple(Vec<String>),
}
