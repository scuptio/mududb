//! C scalar/non-scalar type mappings.
//!
//! The freestanding C guest speaks a three-shape datum wire (see
//! `mudu_sys.h` in the mpm-crate C template): every integer scalar travels
//! as `i64`, every float as `double`, and every string-family scalar
//! (Char/String/Numeric/Date/Time/Timestamp/TimestampTz) as a byte-slice
//! string. The mappings below mirror that surface: all integer widths map
//! to `int64_t` and the string family maps to `mudu_string`. 128-bit integers
//! and blobs have no datum shape in the C guest; they are named here for
//! completeness but rejected by the entity wire classifier
//! (`template_entity_c`) before any code is emitted.

use crate::lang_impl::c::render_c::create_render;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::lang::render::Render;
use crate::{impl_non_scalar, impl_scalar};
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;
use std::sync::Arc;

impl_scalar! {
    c,
    (Bool, "bool"),
    (U8, "int64_t"),
    (U16, "int64_t"),
    (U32, "int64_t"),
    (U64, "int64_t"),
    (U128, "mudu_oid"),
    (I8, "int64_t"),
    (I16, "int64_t"),
    (I32, "int64_t"),
    (I64, "int64_t"),
    (I128, "mudu_oid"),
    (F32, "double"),
    (F64, "double"),
    (Char, "char"),
    (String, "mudu_string"),
    (Blob, "mudu_string"),
    (Numeric, "mudu_string"),
    (Date, "mudu_string"),
    (Time, "mudu_string"),
    (Timestamp, "mudu_string"),
    (TimestampTz, "mudu_string"),
}

impl_non_scalar! {
    c,
    (Array, fn_handle_array),
    (Option, fn_handle_option),
    (Box, fn_handle_box),
    (Tuple, fn_handle_tuple),
}

fn fn_handle_array(inner: &String) -> String {
    format!("const {} *", inner)
}

fn fn_handle_option(inner: &str) -> String {
    // Nullability is carried by the `<field>_is_null` flags of the
    // generated row struct, not by the type.
    inner.to_string()
}

fn fn_handle_box(inner: &String) -> String {
    format!("const {} *", inner)
}

fn fn_handle_tuple(_inner: &[String]) -> String {
    "void *".to_string()
}

/// Create the C rendering back-end.
pub fn create_render_c() -> Arc<dyn Render> {
    create_render()
}

#[cfg(test)]
mod tests {
    use crate::lang_impl::lang::lang_kind::LangKind;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn c_scalar_names_match_the_guest_datum_surface() -> RS<()> {
        let lang = LangKind::C;
        assert_eq!(lang.name_of_scalar(&UniScalar::Bool)?, "bool");
        assert_eq!(lang.name_of_scalar(&UniScalar::I32)?, "int64_t");
        assert_eq!(lang.name_of_scalar(&UniScalar::U64)?, "int64_t");
        assert_eq!(lang.name_of_scalar(&UniScalar::F64)?, "double");
        assert_eq!(lang.name_of_scalar(&UniScalar::Char)?, "char");
        assert_eq!(lang.name_of_scalar(&UniScalar::String)?, "mudu_string");
        assert_eq!(lang.name_of_scalar(&UniScalar::Numeric)?, "mudu_string");
        assert_eq!(lang.name_of_scalar(&UniScalar::Timestamp)?, "mudu_string");
        Ok(())
    }
}
