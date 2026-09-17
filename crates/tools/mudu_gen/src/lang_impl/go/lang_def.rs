//! Go scalar/non-scalar type mappings.
//!
//! The Go guest wire layer (see `mudusys.go` in the mpm-crate Go template)
//! decodes every integer scalar as `int64`, floats as `float64`, the
//! string family as `string`, blobs as `[]byte` and booleans as `bool`;
//! `numeric` is the hand-written `type numeric string` the layer uses to
//! mark the MessagePack Numeric tag. The mappings below mirror that
//! surface: all integer widths map to `int64` and `Numeric` maps to
//! `numeric` (generated entities live in the same `main` package as
//! `mudusys.go`). 128-bit integers have no wire shape in the Go guest;
//! they are named here for completeness but rejected by the entity wire
//! classifier (`template_entity_go`) before any code is emitted.

use crate::lang_impl::go::render_go::create_render;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::lang::render::Render;
use crate::{impl_non_scalar, impl_scalar};
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;
use std::sync::Arc;

impl_scalar! {
    go,
    (Bool, "bool"),
    (U8, "int64"),
    (U16, "int64"),
    (U32, "int64"),
    (U64, "int64"),
    (U128, "muduOid"),
    (I8, "int64"),
    (I16, "int64"),
    (I32, "int64"),
    (I64, "int64"),
    (I128, "muduOid"),
    (F32, "float64"),
    (F64, "float64"),
    (Char, "string"),
    (String, "string"),
    (Blob, "[]byte"),
    (Numeric, "numeric"),
    (Date, "string"),
    (Time, "string"),
    (Timestamp, "string"),
    (TimestampTz, "string"),
}

impl_non_scalar! {
    go,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner: &String) -> String {
    format!("[]{}", inner)
}

fn fn_handle_option(inner: &str) -> String {
    // Nullability is carried by pointer fields of the generated row
    // struct, not by the type.
    format!("*{}", inner)
}

fn fn_handle_box(inner: &String) -> String {
    inner.to_string()
}

fn fn_handle_tuple(_inner: &[String]) -> String {
    "[]any".to_string()
}

/// Create the Go rendering back-end.
pub fn create_render_go() -> Arc<dyn Render> {
    create_render()
}

#[cfg(test)]
mod tests {
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn go_scalar_names_match_the_guest_datum_surface() -> RS<()> {
        let lang = LangKind::Go;
        assert_eq!(lang.name_of_scalar(&UniScalar::Bool)?, "bool");
        assert_eq!(lang.name_of_scalar(&UniScalar::I32)?, "int64");
        assert_eq!(lang.name_of_scalar(&UniScalar::U64)?, "int64");
        assert_eq!(lang.name_of_scalar(&UniScalar::F64)?, "float64");
        assert_eq!(lang.name_of_scalar(&UniScalar::String)?, "string");
        assert_eq!(lang.name_of_scalar(&UniScalar::Blob)?, "[]byte");
        assert_eq!(lang.name_of_scalar(&UniScalar::Numeric)?, "numeric");
        Ok(())
    }
}
