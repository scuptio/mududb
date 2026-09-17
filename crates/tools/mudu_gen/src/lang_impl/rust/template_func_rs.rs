//! Askama template data for Rust MSSP func codecs.
//!
//! The template renders the shared frame preamble ([`TemplateFuncHeaderRS`])
//! and one block of request/result codec stubs per WIT function
//! ([`TemplateFuncRS`]). All language-specific string surgery lives here so
//! the `.jinja` files stay declarative.
//!
//! Wire shapes: a request body is the integer-keyed MessagePack map of the
//! WIT-declared arguments (1-based parameter numbers, proto3 defaults for
//! missing keys); a result body is `[0u8, value]` / `[1u8, error]`. All typed
//! values convert through the `mp_wire` runtime's `Value` model.

use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func, shared_err_type};
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_handle_tuple::lang_handle_tuple;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::rust::value_expr_rs::{rust_from_value, rust_to_value};
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// A `MessageKind` enum case pinned to a WIT function.
pub struct KindInfoRS {
    /// Doc comment of the originating WIT function.
    pub comment: String,
    /// Pascal-case enum case name.
    pub case_name: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// Askama template for the shared Rust MSSP frame preamble.
#[derive(Template)]
#[template(path = "rust/func_header.rs.jinja", escape = "none")]
pub struct TemplateFuncHeaderRS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Message-kind cases, one per WIT function.
    pub kinds: Vec<KindInfoRS>,
    /// Wire error type name shared by all functions (e.g. `UniError`).
    pub err_type_name: String,
}

impl TemplateFuncHeaderRS {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let mut sigs = Vec::with_capacity(funcs.len());
        for func in &funcs {
            sigs.push(analyze_func(func)?);
        }
        let err_ty = shared_err_type(&sigs)?;
        let err_type_name = uni_data_type_to_name(&err_ty, &LangKind::Rust)?;
        let kinds = sigs
            .iter()
            .map(|sig| KindInfoRS {
                comment: sig.func_comments.clone(),
                case_name: to_pascal_case(&sig.func_name),
                discriminant: sig.func_index,
            })
            .collect();
        Ok(Self {
            cfg,
            kinds,
            err_type_name,
        })
    }
}

/// Askama template for one function's Rust MSSP codec stubs.
#[derive(Template)]
#[template(path = "rust/func.rs.jinja", escape = "none")]
pub struct TemplateFuncRS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderRS,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderRS {
    /// Doc comments of the function.
    pub comments: String,
    /// Snake-case function name (`fs_open`).
    pub snake: String,
    /// `MessageKind` case name (`FsOpen`).
    pub kind_case: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Request encoder parameter list (`oid: &UniOid, key: &[u8]`).
    pub req_signature: String,
    /// Request map entries, one `(Value::from(Nu32), <to_value expr>),` per
    /// line.
    pub req_encode_entries: String,
    /// Request decode `let` bindings, one per parameter.
    pub req_decode_stmts: String,
    /// Request decode return expression (`(oid, key)`; `argv` for one
    /// parameter).
    pub req_decode_return: String,
    /// Request decode public return type (`(UniOid, Vec<u8>)`; `UniQueryArgv`
    /// for one parameter; `()` for none).
    pub req_return_type: String,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Public ok type of the result (`Option<Vec<u8>>`; `()` for unit).
    pub res_ok_type: String,
    /// Closure body mapping `&T` (via `Borrow<OkT>`) to its wire `Value` on
    /// encode.
    pub res_encode_map: String,
    /// Expression converting the decoded ok `Value` (referenced as `&value`)
    /// into `Result<OkT, WireError>`.
    pub res_decode_expr: String,
}

impl TemplateFuncRS {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        Ok(Self {
            cfg,
            func: FuncRenderRS::from_sig(&sig)?,
        })
    }
}

impl FuncRenderRS {
    fn from_sig(sig: &FuncSig) -> RS<Self> {
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut encode_entries = Vec::with_capacity(sig.params.len());
        let mut decode_stmts = Vec::with_capacity(sig.params.len());
        let mut decode_returns = Vec::with_capacity(sig.params.len());
        let mut return_types = Vec::with_capacity(sig.params.len());
        for (i, param) in sig.params.iter().enumerate() {
            let number = i + 1;
            let name = to_snake_case(&param.param_name);
            let param_ty = rust_param_type(&param.param_type)?;
            signature.push(format!("{}: {}", name, param_ty));
            // The value-expression convention takes a `&T` place: scalar
            // parameters arrive by value, everything else by reference.
            let place = if param_ty.starts_with('&') {
                name.clone()
            } else {
                format!("&{name}")
            };
            encode_entries.push(format!(
                "            (Value::from({}u32), {}),",
                number,
                rust_to_value(&param.param_type, &place, true)?
            ));
            decode_stmts.push(format!(
                "    let {} = decode_field(&fields, {}, |v: &Value| {})?;",
                name,
                number,
                rust_from_value(&param.param_type, "v")?
            ));
            decode_returns.push(name.clone());
            return_types.push(uni_data_type_to_name(&param.param_type, &LangKind::Rust)?);
        }
        let param_count = sig.params.len();
        let req_return_type = match param_count {
            0 => "()".to_string(),
            1 => return_types[0].clone(),
            _ => lang_handle_tuple(&return_types),
        };
        let req_decode_return = match param_count {
            0 => "()".to_string(),
            1 => decode_returns[0].clone(),
            _ => lang_handle_tuple(&decode_returns),
        };
        let (result_is_unit, res_ok_type, res_encode_map, res_decode_expr) = match &sig.result {
            FuncResult::Unit { .. } => (true, "()".to_string(), String::new(), String::new()),
            FuncResult::Value { ok, .. } => {
                let ok_type = uni_data_type_to_name(ok, &LangKind::Rust)?;
                // Fully-qualified `Borrow::borrow` keeps the encode map
                // unambiguous for both owned (`T`) and borrowed (`&T`)
                // success values accepted by the generic result encoder.
                let borrow_expr = format!("core::borrow::Borrow::<{}>::borrow(value)", ok_type);
                (
                    false,
                    ok_type,
                    rust_to_value(ok, &borrow_expr, true)?,
                    rust_from_value(ok, "&value")?,
                )
            }
        };
        Ok(Self {
            comments: sig.func_comments.clone(),
            snake: to_snake_case(&sig.func_name),
            kind_case: to_pascal_case(&sig.func_name),
            param_count,
            req_signature: signature.join(", "),
            req_encode_entries: encode_entries.join("\n"),
            req_decode_stmts: decode_stmts.join("\n"),
            req_decode_return,
            req_return_type,
            result_is_unit,
            res_ok_type,
            res_encode_map,
            res_decode_expr,
        })
    }
}

/// The Rust parameter type used in generated request-encoder signatures.
///
/// Scalars go by value; strings, records, byte blobs and composites go by
/// reference. (Signatures differ slightly from the historical hand-written
/// router, which mixed by-value and by-reference records; the wire bytes are
/// identical either way.)
fn rust_param_type(ty: &UniDataType) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::String
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "&str".to_string(),
            UniScalar::Blob => "&[u8]".to_string(),
            UniScalar::Numeric => {
                format!("&{}", uni_data_type_to_name(ty, &LangKind::Rust)?)
            }
            _ => uni_data_type_to_name(ty, &LangKind::Rust)?,
        },
        UniDataType::Binary => "&[u8]".to_string(),
        UniDataType::Identifier(name) => format!("&{}", to_pascal_case(name)),
        UniDataType::Array(_) | UniDataType::Option(_) | UniDataType::Tuple(_) => {
            format!("&{}", uni_data_type_to_name(ty, &LangKind::Rust)?)
        }
        UniDataType::Box(inner) => rust_param_type(inner)?,
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            uni_data_type_to_name(ty, &LangKind::Rust)?
        }
    };
    Ok(s)
}
