//! Language dispatch functions for scalar / non-scalar type names.

use crate::impl_lang;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::non_scalar::NonScalarType;
use crate::lang_impl::{assemblyscript, c, csharp, go, python, rust};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use paste::paste;

impl_lang! {
    (Rust, rust),
    (CSharp, csharp),
    (AssemblyScript, assemblyscript),
    (Python, python),
    (C, c),
    (Go, go),
}

/// Convert a [`UniDataType`] into a language-specific type name.
pub fn uni_data_type_to_name(wit_ty: &UniDataType, lang: &LangKind) -> RS<String> {
    _to_lang_type(wit_ty, lang)
}

fn to_scalar_type(wit_prim: &UniScalar, lang: &LangKind) -> RS<String> {
    Ok(lang_scalar_name(lang, wit_prim))
}

/// The unit type name used for absent `result` arms.
fn unit_type_name(lang: &LangKind) -> String {
    match lang {
        LangKind::Rust => "()".to_string(),
        LangKind::CSharp => "byte".to_string(),
        LangKind::AssemblyScript => "bool".to_string(),
        LangKind::Python => "None".to_string(),
        LangKind::C => "void".to_string(),
        LangKind::Go => "struct{}".to_string(),
    }
}

fn to_non_scalar_type(non_scalar: &NonScalarType, lang: &LangKind) -> RS<String> {
    Ok(lang_non_scalar_name(lang, non_scalar))
}

fn handle_wit_tuple(vec_wit_ty: &[UniDataType], lang: &LangKind) -> RS<String> {
    let mut vec = Vec::new();
    for wit_ty in vec_wit_ty.iter() {
        let ty = uni_data_type_to_name(wit_ty, lang)?;
        vec.push(ty);
    }
    let non_scalar = NonScalarType::Tuple(vec);
    let s = to_non_scalar_type(&non_scalar, lang)?;
    Ok(s)
}

fn _to_lang_type(wit_ty: &UniDataType, lang: &LangKind) -> RS<String> {
    let ty_str = match wit_ty {
        UniDataType::Scalar(p_ty) => to_scalar_type(p_ty, lang)?,
        UniDataType::Tuple(vec) => handle_wit_tuple(vec, lang)?,
        UniDataType::Array(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Array(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDataType::Option(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            if *lang == LangKind::CSharp {
                format!("{}?", inner)
            } else {
                let non_scalar = NonScalarType::Option(inner);
                to_non_scalar_type(&non_scalar, lang)?
            }
        }
        UniDataType::Identifier(ty_name) => to_pascal_case(ty_name),
        UniDataType::Box(inner_ty) => {
            let inner = uni_data_type_to_name(inner_ty, lang)?;
            let non_scalar = NonScalarType::Box(inner);
            to_non_scalar_type(&non_scalar, lang)?
        }
        UniDataType::Binary => match lang {
            // Struct-context `list<u8>`: a plain MessagePack ARRAY of u8 (the
            // Rust host encodes `Vec<u8>` record fields as arrays, not bin).
            // `List<byte>` matches that shape with MessagePack-CSharp;
            // `byte[]` would encode as bin. Func-level `list<u8>` blobs are
            // `byte[]` (bin) and are mapped by the func templates instead.
            LangKind::CSharp => "List<byte>".to_string(),
            _ => to_scalar_type(&UniScalar::Blob, lang)?,
        },
        UniDataType::Result(result_ty) => {
            let ok_name = match &result_ty.ok {
                Some(ok_ty) => uni_data_type_to_name(ok_ty, lang)?,
                None => unit_type_name(lang),
            };
            match lang {
                LangKind::Rust => {
                    let err_name = match &result_ty.err {
                        Some(err_ty) => uni_data_type_to_name(err_ty, lang)?,
                        None => unit_type_name(lang),
                    };
                    format!("Result<{}, {}>", ok_name, err_name)
                }
                LangKind::CSharp => {
                    // Mirrors the generated C# decode signatures: a value
                    // result decodes to `SyscallResult<T>`, a unit result to
                    // the carried `UniError?` (null on success).
                    if result_ty.ok.is_some() {
                        format!("SyscallResult<{}>", ok_name)
                    } else {
                        "UniError?".to_string()
                    }
                }
                LangKind::AssemblyScript => {
                    // The func template generates per-function result holder
                    // classes; the generic mapping falls back to the ok type
                    // name (`bool` placeholder for unit results).
                    ok_name
                }
                LangKind::Python => {
                    // Python funcs return the untyped `WireResult` holder; the
                    // generic mapping falls back to the ok type name.
                    ok_name
                }
                LangKind::C | LangKind::Go => {
                    // C/Go generation covers entities only; fall back to the
                    // ok type name for the (unsupported) func result mapping.
                    ok_name
                }
            }
        }
        UniDataType::Struct { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "record type is not implemented for language code generation"
            ));
        }
    };
    Ok(ty_str)
}

impl LangKind {
    /// Return the language-specific name of a scalar type.
    pub fn name_of_scalar(&self, p: &UniScalar) -> RS<String> {
        Ok(lang_scalar_name(self, p))
    }

    /// Return the language-specific name of a WIT data type.
    pub fn name_of_wit_type(&self, wit_type: &UniDataType) -> RS<String> {
        uni_data_type_to_name(wit_type, self)
    }
}
