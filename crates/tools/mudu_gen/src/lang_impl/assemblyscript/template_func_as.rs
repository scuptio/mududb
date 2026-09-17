//! Askama template data for AssemblyScript MSSP func codecs.
//!
//! Renders the shared frame preamble ([`TemplateFuncHeaderAS`]: `MessageKind`
//! enum plus the 16-byte header/frame helpers) and one block of request /
//! result codec stubs per WIT function ([`TemplateFuncAS`]). All statements
//! use the explicit `MpackWriter`/`MpackReader` methods of the `mpack.ts`
//! runtime via [`crate::lang_impl::assemblyscript::as_codec`].

use crate::lang_impl::assemblyscript::as_codec::{
    AS_STYLE_FUNC, AsTupleHolder, CollectingTupleNamer, as_codec_default, as_codec_type,
    as_decode_stmts, as_encode_stmts, as_join,
};
use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func};
use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;

/// A `MessageKind` enum case pinned to a WIT function.
pub struct KindInfoAS {
    /// Doc comment of the originating WIT function.
    pub comment: String,
    /// Pascal-case enum case name.
    pub case_name: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// Askama template for the shared AssemblyScript MSSP frame preamble.
#[derive(Template)]
#[template(path = "assemblyscript/func_header.ts.jinja", escape = "none")]
pub struct TemplateFuncHeaderAS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Message-kind cases, one per WIT function.
    pub kinds: Vec<KindInfoAS>,
}

impl TemplateFuncHeaderAS {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let mut kinds = Vec::with_capacity(funcs.len());
        for func in &funcs {
            let sig = analyze_func(func)?;
            kinds.push(KindInfoAS {
                comment: sig.func_comments.clone(),
                case_name: to_pascal_case(&sig.func_name),
                discriminant: sig.func_index,
            });
        }
        Ok(Self { cfg, kinds })
    }
}

/// A field of a generated request holder class (funcs with > 1 parameter).
pub struct HolderFieldAS {
    /// camelCase field name.
    pub name: String,
    /// AssemblyScript field type.
    pub ty: String,
    /// Default-value expression.
    pub default: String,
}

/// Per-parameter request decode data: the 1-based parameter number (the wire
/// map key) and the statements decoding the value into the parameter target.
pub struct ReqParamAS {
    /// 1-based parameter number.
    pub number: usize,
    /// Decode statements (joined, indented for the `case` block body).
    pub decode_stmts: String,
}

/// Askama template for one function's AssemblyScript MSSP codec stubs.
#[derive(Template)]
#[template(path = "assemblyscript/func.ts.jinja", escape = "none")]
pub struct TemplateFuncAS {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderAS,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderAS {
    /// Doc comments of the function.
    pub comments: String,
    /// Pascal-case function name (`FsOpen`).
    pub pascal: String,
    /// `MessageKind` case name (`FsOpen`).
    pub kind_case: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Request encoder parameter list (`oid: UniOid, key: Uint8Array`).
    pub req_signature: String,
    /// Request encode statements (joined).
    pub req_encode_stmts: String,
    /// Request holder class name when `param_count > 1`.
    pub req_holder_name: String,
    /// Request holder class fields (when `param_count > 1`).
    pub req_holder_fields: Vec<HolderFieldAS>,
    /// Single-parameter decode type and decode statements.
    pub req_single_type: String,
    /// Single-parameter decode default expression.
    pub req_single_default: String,
    /// Per-parameter request decode data, in declaration order.
    pub req_params: Vec<ReqParamAS>,
    /// Generated tuple holder classes referenced by this function.
    pub tuple_holders: Vec<AsTupleHolder>,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Result holder class name (`GetResult`).
    pub res_holder_name: String,
    /// Ok value type (`Uint8Array | null`; empty for unit results).
    pub res_ok_type: String,
    /// Ok value default expression.
    pub res_ok_default: String,
    /// Statements encoding the ok value (joined).
    pub res_encode_stmts: String,
    /// Statements decoding the ok value (joined).
    pub res_decode_stmts: String,
    /// Wire error type name (e.g. `UniError`).
    pub err_type_name: String,
}

impl TemplateFuncAS {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        Ok(Self {
            func: FuncRenderAS::from_sig(&sig, cfg.type_kinds.clone())?,
            cfg,
        })
    }
}

fn to_camel_case(name: &str) -> String {
    let pascal = to_pascal_case(name);
    let mut chars = pascal.chars();
    match chars.next() {
        Some(first) => first.to_lowercase().chain(chars).collect(),
        None => pascal,
    }
}

impl FuncRenderAS {
    fn from_sig(sig: &FuncSig, kinds: std::collections::HashMap<String, String>) -> RS<Self> {
        let pascal = to_pascal_case(&sig.func_name);
        let err_ty = match &sig.result {
            FuncResult::Unit { err } => err.clone(),
            FuncResult::Value { err, .. } => err.clone(),
        };
        let err_type_name = uni_data_type_to_name(&err_ty, &LangKind::AssemblyScript)?;
        let mut namer = CollectingTupleNamer::with_kinds(kinds);

        // ---- request side ----
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut encode_lines = vec![format!(
            "        writer.writeMapHeader({});",
            sig.params.len()
        )];
        let mut req_params = Vec::with_capacity(sig.params.len());
        let mut holder_fields = Vec::with_capacity(sig.params.len());
        let mut req_single_type = String::new();
        let mut req_single_default = String::new();
        let multi = sig.params.len() > 1;
        for (index, param) in sig.params.iter().enumerate() {
            let camel = to_camel_case(&param.param_name);
            let base = format!("{}{}", pascal, to_pascal_case(&param.param_name));
            namer.begin(base);
            let ty = as_codec_type(&param.param_type, &mut namer)?;
            signature.push(format!("{}: {}", camel, ty));
            encode_lines.push(format!("        writer.writeU64({});", index + 1));
            as_encode_stmts(
                AS_STYLE_FUNC,
                &param.param_type,
                &camel,
                8,
                &mut encode_lines,
            )?;
            namer.begin(format!("{}{}", pascal, to_pascal_case(&param.param_name)));
            let target = if multi {
                format!("value.{}", camel)
            } else {
                "value".to_string()
            };
            let mut decode_lines = Vec::new();
            as_decode_stmts(
                AS_STYLE_FUNC,
                &param.param_type,
                &target,
                12,
                &mut decode_lines,
                &mut namer,
            )?;
            req_params.push(ReqParamAS {
                number: index + 1,
                decode_stmts: as_join(&decode_lines),
            });
            let default = as_codec_default(&param.param_type, &mut namer)?;
            if multi {
                holder_fields.push(HolderFieldAS {
                    name: camel,
                    ty,
                    default,
                });
            } else {
                req_single_type = ty;
                req_single_default = default;
            }
        }

        // ---- result side ----
        let (result_is_unit, res_ok_type, res_ok_default, res_encode_lines, res_decode_lines) =
            match &sig.result {
                FuncResult::Unit { .. } => (true, String::new(), String::new(), None, None),
                FuncResult::Value { ok, .. } => {
                    namer.begin(format!("{}Result", pascal));
                    let ok_type = as_codec_type(ok, &mut namer)?;
                    let ok_default = as_codec_default(ok, &mut namer)?;
                    let mut enc = Vec::new();
                    as_encode_stmts(AS_STYLE_FUNC, ok, "result.value", 12, &mut enc)?;
                    namer.begin(format!("{}Result", pascal));
                    let mut dec = Vec::new();
                    as_decode_stmts(AS_STYLE_FUNC, ok, "result.value", 12, &mut dec, &mut namer)?;
                    (false, ok_type, ok_default, Some(enc), Some(dec))
                }
            };
        Ok(Self {
            comments: sig.func_comments.clone(),
            kind_case: pascal.clone(),
            pascal: pascal.clone(),
            param_count: sig.params.len(),
            req_signature: signature.join(", "),
            req_encode_stmts: as_join(&encode_lines),
            req_holder_name: format!("{}Request", pascal),
            req_holder_fields: holder_fields,
            req_single_type,
            req_single_default,
            req_params,
            tuple_holders: namer.holders().to_vec(),
            result_is_unit,
            res_holder_name: format!("{}Result", pascal),
            res_ok_type,
            res_ok_default,
            res_encode_stmts: res_encode_lines.as_deref().map_or(String::new(), as_join),
            res_decode_stmts: res_decode_lines.as_deref().map_or(String::new(), as_join),
            err_type_name,
        })
    }
}
