//! Askama template data for C# MSSP func codecs.
//!
//! Renders the shared frame preamble ([`TemplateFuncHeaderCS`]: `MessageKind`
//! enum, `SyscallResult<T>` and the `SyscallPayload` frame codec, mirroring
//! the historical hand-written `SyscallPayload.cs`) and one `partial` static
//! class block of request/result stubs per WIT function ([`TemplateFuncCS`]).

use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func, shared_err_type};
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_handle_tuple::lang_handle_tuple;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;

/// A `MessageKind` enum case pinned to a WIT function.
pub struct KindInfoCS {
    /// Doc comment of the originating WIT function.
    pub comment: String,
    /// Pascal-case enum case name.
    pub case_name: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// Askama template for the shared C# MSSP frame preamble.
#[derive(Template)]
#[template(path = "csharp/func_header.cs.jinja", escape = "none")]
pub struct TemplateFuncHeaderCS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Message-kind cases, one per WIT function.
    pub kinds: Vec<KindInfoCS>,
    /// Wire error type name shared by all functions (e.g. `UniError`).
    pub err_type_name: String,
    /// All user-defined type names known to this generation run (sorted):
    /// the generated `UniCodec` statically roots their formatters so the
    /// resolver-free dispatch survives IL trimming/AOT.
    pub codec_types: Vec<String>,
}

impl TemplateFuncHeaderCS {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let mut sigs = Vec::with_capacity(funcs.len());
        for func in &funcs {
            sigs.push(analyze_func(func)?);
        }
        let err_ty = shared_err_type(&sigs)?;
        let err_type_name = uni_data_type_to_name(&err_ty, &LangKind::CSharp)?;
        let kinds = sigs
            .iter()
            .map(|sig| KindInfoCS {
                comment: sig.func_comments.clone(),
                case_name: to_pascal_case(&sig.func_name),
                discriminant: sig.func_index,
            })
            .collect();
        let mut codec_types: Vec<String> = cfg.type_kinds.keys().cloned().collect();
        codec_types.sort();
        Ok(Self {
            cfg,
            kinds,
            err_type_name,
            codec_types,
        })
    }
}

/// Askama template for one function's C# MSSP codec stubs.
#[derive(Template)]
#[template(path = "csharp/func.cs.jinja", escape = "none")]
pub struct TemplateFuncCS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderCS,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderCS {
    /// Doc comments of the function.
    pub comments: String,
    /// Pascal-case function name (`FsOpen`).
    pub pascal: String,
    /// `MessageKind` case name (`FsOpen`).
    pub kind_case: String,
    /// Name of the static partial class holding the stubs.
    pub module_name: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Request encoder parameter list (`UniOid oid, byte[] key`).
    pub req_signature: String,
    /// Comma-separated argument list forwarded to `EncodeRequestFrame`.
    pub req_args: String,
    /// Request decode generic argument list (`UniOid, byte[]`).
    pub req_decode_types: String,
    /// Request decode public return type (`(UniOid, byte[])`; `UniQueryArgv`
    /// for one parameter).
    pub req_return_type: String,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Public ok type of the result (`byte[]`; empty for unit).
    pub res_ok_type: String,
    /// Wire error type name (e.g. `UniError`).
    pub err_type_name: String,
}

impl TemplateFuncCS {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        let module_name = if cfg.func_module_name.is_empty() {
            "Syscall".to_string()
        } else {
            cfg.func_module_name.clone()
        };
        let err_ty = match &sig.result {
            FuncResult::Unit { err } => err.clone(),
            FuncResult::Value { err, .. } => err.clone(),
        };
        let err_type_name = uni_data_type_to_name(&err_ty, &LangKind::CSharp)?;
        Ok(Self {
            func: FuncRenderCS::from_sig(&sig, module_name, err_type_name)?,
            cfg,
        })
    }
}

impl FuncRenderCS {
    fn from_sig(sig: &FuncSig, module_name: String, err_type_name: String) -> RS<Self> {
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut args = Vec::with_capacity(sig.params.len());
        let mut decode_types = Vec::with_capacity(sig.params.len());
        let mut return_types = Vec::with_capacity(sig.params.len());
        for param in &sig.params {
            let name = to_camel_case(&param.param_name);
            let ty = csharp_func_type(&param.param_type)?;
            signature.push(format!("{} {}", ty, name));
            args.push(name);
            decode_types.push(ty.clone());
            return_types.push(ty);
        }
        let param_count = sig.params.len();
        let req_return_type = match param_count {
            0 => "void".to_string(),
            1 => return_types[0].clone(),
            _ => lang_handle_tuple(&return_types),
        };
        let (result_is_unit, res_ok_type) = match &sig.result {
            FuncResult::Unit { .. } => (true, String::new()),
            FuncResult::Value { ok, .. } => (false, csharp_func_type(ok)?),
        };
        Ok(Self {
            comments: sig.func_comments.clone(),
            pascal: to_pascal_case(&sig.func_name),
            kind_case: to_pascal_case(&sig.func_name),
            module_name,
            param_count,
            req_signature: signature.join(", "),
            req_args: args.join(", "),
            req_decode_types: decode_types.join(", "),
            req_return_type,
            result_is_unit,
            res_ok_type,
            err_type_name,
        })
    }
}

/// camelCase rendering of a WIT kebab-case name for C# parameter names.
fn to_camel_case(name: &str) -> String {
    let pascal = to_pascal_case(name);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => pascal,
    }
}

/// The C# type of a func-level value: `list<u8>` blobs are `byte[]`
/// (MessagePack-CSharp encodes `byte[]` as MessagePack **bin**), in contrast
/// to record fields where `list<u8>` maps to `List<byte>` (MessagePack
/// **array**).
fn csharp_func_type(ty: &UniDataType) -> RS<String> {
    let s = match ty {
        UniDataType::Binary => "byte[]".to_string(),
        UniDataType::Array(inner) => {
            let inner = csharp_func_type(inner)?;
            if inner == "byte" {
                "byte[]".to_string()
            } else {
                format!("List<{}>", inner)
            }
        }
        UniDataType::Option(inner) => format!("{}?", csharp_func_type(inner)?),
        UniDataType::Tuple(elems) => {
            let mut names = Vec::with_capacity(elems.len());
            for elem in elems {
                names.push(csharp_func_type(elem)?);
            }
            lang_handle_tuple(&names)
        }
        UniDataType::Box(inner) => csharp_func_type(inner)?,
        _ => uni_data_type_to_name(ty, &LangKind::CSharp)?,
    };
    Ok(s)
}
