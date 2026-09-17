//! Askama template data for Go MSSP func codecs.
//!
//! Renders the shared frame preamble ([`TemplateFuncHeaderGo`]: the
//! `MessageKind` const block, the `WireResult` holder and the header decode
//! wrapper) and one block of request / result codec functions per WIT
//! function ([`TemplateFuncGo`]). All typed values convert through the value
//! model of [`crate::lang_impl::go::go_codec`]; the frames are serialized by
//! the hand-written `codec` package of the Go binding.

use crate::lang_impl::go::go_codec::{
    GO_FUNC_BIN, go_comments, go_from_value, go_local_ident, go_to_value, go_type_name,
    go_variant_default, go_wire_from_any, indent_comments,
};
use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func, shared_err_type};
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;

/// A `MessageKind` const pinned to a WIT function.
pub struct KindInfoGo {
    /// `//`-style doc comments, indented for the const block.
    pub comments: String,
    /// Exported const name (`MessageKindFsOpen`).
    pub member_name: String,
    /// Const name padded inside its comment-free run (gofmt alignment).
    pub member_name_padded: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// Askama template for the shared Go MSSP frame preamble.
#[derive(Template)]
#[template(path = "go/func_header.go.jinja", escape = "none")]
pub struct TemplateFuncHeaderGo {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Message-kind consts, one per WIT function.
    pub kinds: Vec<KindInfoGo>,
    /// Comma-separated const names (the DecodeHeader accept list).
    pub member_list: String,
    /// Go type name of the shared error type (`UniError`).
    pub err_type_name: String,
}

impl TemplateFuncHeaderGo {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let mut sigs = Vec::with_capacity(funcs.len());
        for func in &funcs {
            sigs.push(analyze_func(func)?);
        }
        let err_ty = shared_err_type(&sigs)?;
        let names: Vec<String> = sigs
            .iter()
            .map(|sig| format!("MessageKind{}", to_pascal_case(&sig.func_name)))
            .collect();
        let has_comments: Vec<bool> = sigs
            .iter()
            .map(|sig| !sig.func_comments.is_empty())
            .collect();
        let padded = crate::lang_impl::go::go_codec::pad_ident_runs(&has_comments, &names);
        let kinds: Vec<KindInfoGo> = sigs
            .iter()
            .enumerate()
            .map(|(i, sig)| KindInfoGo {
                comments: indent_comments(&go_comments(&sig.func_comments)),
                member_name: names[i].clone(),
                member_name_padded: padded[i].clone(),
                discriminant: sig.func_index,
            })
            .collect();
        let member_list = kinds
            .iter()
            .map(|kind| kind.member_name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        Ok(Self {
            cfg,
            kinds,
            member_list,
            err_type_name: go_type_name(&err_ty)?,
        })
    }
}

/// Askama template for one function's Go MSSP codec functions.
#[derive(Template)]
#[template(path = "go/func.go.jinja", escape = "none")]
pub struct TemplateFuncGo {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderGo,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderGo {
    /// `//`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Pascal-case function name (`FsOpen`).
    pub pascal: String,
    /// Kebab-case function name as declared in WIT (`fs-open`).
    pub kebab: String,
    /// `MessageKind` const name (`MessageKindFsOpen`).
    pub kind_member: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Request encoder parameter list (`oid UniOid, key []byte`).
    pub req_signature: String,
    /// Request decoder result declarations, one `var name Type` per line.
    pub req_result_decls: String,
    /// Default-seeding statements for variant-typed parameters (empty when
    /// none; every other type's Go zero value matches the wire default).
    pub req_default_inits: String,
    /// Request encode statements, one block per parameter assigning `_body[N]`.
    pub req_encode_stmts: String,
    /// Request decode statements, one guarded block per parameter.
    pub req_decode_stmts: String,
    /// Comma-separated result type list (`UniOid, []byte`).
    pub req_return_types: String,
    /// Comma-separated result name list (`oid, key`).
    pub req_return_names: String,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Pre-rendered statements converting `result.Value` to the wire value
    /// `_w` (empty for unit results).
    pub res_ok_encode_stmts: String,
    /// Pre-rendered statements decoding `_payload` into the typed `_v`
    /// (empty for unit results).
    pub res_ok_decode_stmts: String,
    /// Go type name of the shared error type (`UniError`).
    pub err_type_name: String,
}

impl TemplateFuncGo {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        let err_ty = match &sig.result {
            FuncResult::Unit { err } => err.clone(),
            FuncResult::Value { err, .. } => err.clone(),
        };
        Ok(Self {
            func: FuncRenderGo::from_sig(&sig, &cfg, &err_ty)?,
            cfg,
        })
    }
}

impl FuncRenderGo {
    fn from_sig(sig: &FuncSig, cfg: &CodegenCfg, err_ty: &UniDataType) -> RS<Self> {
        let mut tmp = 0;
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut result_decls = Vec::with_capacity(sig.params.len());
        let mut default_inits = Vec::new();
        let mut encode_stmts = Vec::with_capacity(sig.params.len());
        let mut decode_stmts = Vec::with_capacity(sig.params.len());
        let mut return_types = Vec::with_capacity(sig.params.len());
        let mut return_names = Vec::with_capacity(sig.params.len());
        for param in &sig.params {
            let name = go_local_ident(&param.param_name);
            let type_name = go_type_name(&param.param_type)?;
            signature.push(format!("{name} {type_name}"));
            result_decls.push(format!("\tvar {name} {type_name}"));
            if let Some(expr) = go_variant_default(&param.param_type, &cfg.type_kinds)? {
                default_inits.push(format!("\t{name} = {expr}"));
            }
            return_types.push(type_name);
            return_names.push(name);
        }
        // The decoder error return must name every result, so it is computed
        // only after all parameter names are known.
        let err_ret = format!("return {}, err", return_names.join(", "));
        for (index, param) in sig.params.iter().enumerate() {
            let number = index + 1;
            let name = return_names[index].clone();
            encode_stmts.push(go_to_value(
                &param.param_type,
                &name,
                &format!("_body[{number}]"),
                &mut tmp,
                "\t",
                GO_FUNC_BIN,
            )?);
            decode_stmts.push(format!(
                "\tif _raw, _has := _fields[{number}]; _has {{\n{}\n\t}}",
                go_from_value(&param.param_type, "_raw", &name, &mut tmp, "\t\t", &err_ret)?
            ));
        }
        let param_count = sig.params.len();
        let (result_is_unit, res_ok_encode_stmts, res_ok_decode_stmts) = match &sig.result {
            FuncResult::Unit { .. } => (true, String::new(), String::new()),
            FuncResult::Value { ok, .. } => {
                let encode =
                    go_wire_from_any(ok, "result.Value", "_w", &mut tmp, "\t", GO_FUNC_BIN)?;
                let decode = match ok {
                    // WireResult.Value carries option payloads unwrapped:
                    // nil decodes as an absent value, anything else as the
                    // typed inner value.
                    UniDataType::Option(inner) => format!(
                        "\tvar _v any\n\tif _payload != nil {{\n\t\tvar _t {}\n{}\n\t\t_v = _t\n\t}}",
                        go_type_name(inner)?,
                        go_from_value(
                            inner,
                            "_payload",
                            "_t",
                            &mut tmp,
                            "\t\t",
                            "return WireResult{}, err"
                        )?
                    ),
                    _ => format!(
                        "\tvar _v {}\n{}",
                        go_type_name(ok)?,
                        go_from_value(
                            ok,
                            "_payload",
                            "_v",
                            &mut tmp,
                            "\t",
                            "return WireResult{}, err"
                        )?
                    ),
                };
                (false, encode, decode)
            }
        };
        Ok(Self {
            comments: go_comments(&sig.func_comments),
            pascal: to_pascal_case(&sig.func_name),
            kebab: sig.func_name.clone(),
            kind_member: format!("MessageKind{}", to_pascal_case(&sig.func_name)),
            param_count,
            req_signature: signature.join(", "),
            req_result_decls: result_decls.join("\n"),
            req_default_inits: default_inits.join("\n"),
            req_encode_stmts: encode_stmts.join("\n"),
            req_decode_stmts: decode_stmts.join("\n"),
            req_return_types: return_types.join(", "),
            req_return_names: return_names.join(", "),
            result_is_unit,
            res_ok_encode_stmts,
            res_ok_decode_stmts,
            err_type_name: go_type_name(err_ty)?,
        })
    }
}
