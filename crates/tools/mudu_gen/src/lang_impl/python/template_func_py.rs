//! Askama template data for Python MSSP func codecs.
//!
//! Renders the shared frame preamble ([`TemplateFuncHeaderPy`]: the
//! `MessageKind` IntEnum, the 16-byte header/frame helpers, the `WireResult`
//! holder and the request/result body helpers) and one block of request /
//! result codec functions per WIT function ([`TemplateFuncPy`]). All typed
//! values convert through the native value model of
//! [`crate::lang_impl::python::py_codec`]; the bodies are serialized by the
//! generic `MpackWriter.write_value` / `MpackReader.read_value` of the
//! hand-written `mududb.codec.mpack` runtime.

use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func, shared_err_type};
use crate::lang_impl::python::py_codec::{
    PY_FUNC_BIN, py_comments, py_default_factory, py_from_value, py_ident, py_to_value,
    py_type_name,
};
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_snake_case, to_snake_case_upper};
use std::collections::HashMap;

/// A `MessageKind` enum member pinned to a WIT function.
pub struct KindInfoPy {
    /// `#`-style doc comments, indented for the enum body.
    pub comments: String,
    /// UPPER_SNAKE enum member name.
    pub member_name: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// Askama template for the shared Python MSSP frame preamble.
#[derive(Template)]
#[template(path = "python/func_header.py.jinja", escape = "none")]
pub struct TemplateFuncHeaderPy {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Message-kind members, one per WIT function.
    pub kinds: Vec<KindInfoPy>,
    /// Wire-value conversion expression of the shared error type on
    /// `result.error`.
    pub err_to_value_expr: String,
    /// Typed-value conversion expression of the shared error type on
    /// `value[1]`.
    pub err_from_value_expr: String,
}

impl TemplateFuncHeaderPy {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let mut sigs = Vec::with_capacity(funcs.len());
        for func in &funcs {
            sigs.push(analyze_func(func)?);
        }
        let err_ty = shared_err_type(&sigs)?;
        let kinds = sigs
            .iter()
            .map(|sig| KindInfoPy {
                comments: crate::lang_impl::python::template_record_py::indent_comments(
                    &py_comments(&sig.func_comments),
                    4,
                ),
                member_name: to_snake_case_upper(&sig.func_name),
                discriminant: sig.func_index,
            })
            .collect();
        Ok(Self {
            cfg,
            kinds,
            err_to_value_expr: py_to_value(&err_ty, "result.error", PY_FUNC_BIN)?,
            err_from_value_expr: py_from_value(&err_ty, "value[1]")?,
        })
    }
}

/// Askama template for one function's Python MSSP codec functions.
#[derive(Template)]
#[template(path = "python/func.py.jinja", escape = "none")]
pub struct TemplateFuncPy {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderPy,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderPy {
    /// `#`-style doc comments (empty when undocumented).
    pub comments: String,
    /// Snake-case function name (`fs_open`).
    pub snake: String,
    /// `MessageKind` member name (`FS_OPEN`).
    pub kind_member: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Request encoder parameter list (`oid: UniOid, key: bytes`).
    pub req_signature: String,
    /// Request map entries, one `N: <to_value expr>,` per line.
    pub req_encode_entries: String,
    /// Request decode statements, one assignment per parameter.
    pub req_decode_stmts: String,
    /// Request decode return expression (`(oid, key)`; `oid` for one
    /// parameter).
    pub req_decode_return: String,
    /// Request decode return annotation (`tuple[UniOid, bytes]`;
    /// `UniOid` for one parameter; `None` for none).
    pub req_return_type: String,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Wire-value conversion expression of the ok payload on `value`
    /// (empty for unit results).
    pub res_ok_to_value: String,
    /// Typed-value conversion expression of the ok payload on `raw.value`
    /// (empty for unit results).
    pub res_ok_from_value: String,
}

impl TemplateFuncPy {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        Ok(Self {
            func: FuncRenderPy::from_sig(&sig, &cfg.type_kinds)?,
            cfg,
        })
    }
}

impl FuncRenderPy {
    fn from_sig(sig: &FuncSig, type_kinds: &HashMap<String, String>) -> RS<Self> {
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut encode_entries = Vec::with_capacity(sig.params.len());
        let mut decode_stmts = Vec::with_capacity(sig.params.len());
        let mut decode_returns = Vec::with_capacity(sig.params.len());
        let mut return_types = Vec::with_capacity(sig.params.len());
        for (index, param) in sig.params.iter().enumerate() {
            let number = index + 1;
            let name = py_ident(&to_snake_case(&param.param_name));
            signature.push(format!("{}: {}", name, py_type_name(&param.param_type)?));
            encode_entries.push(format!(
                "            {}: {},",
                number,
                py_to_value(&param.param_type, &name, PY_FUNC_BIN)?
            ));
            decode_stmts.push(format!(
                "    {} = decode_field(fields, {}, lambda _v: {}, {})",
                name,
                number,
                py_from_value(&param.param_type, "_v")?,
                py_default_factory(&param.param_type, type_kinds)?
            ));
            decode_returns.push(name);
            return_types.push(py_type_name(&param.param_type)?);
        }
        let param_count = sig.params.len();
        let (req_return_type, req_decode_return) = match param_count {
            0 => ("None".to_string(), "None".to_string()),
            1 => (return_types[0].clone(), decode_returns[0].clone()),
            _ => (
                format!("tuple[{}]", return_types.join(", ")),
                format!("({})", decode_returns.join(", ")),
            ),
        };
        let (result_is_unit, res_ok_to_value, res_ok_from_value) = match &sig.result {
            FuncResult::Unit { .. } => (true, String::new(), String::new()),
            FuncResult::Value { ok, .. } => (
                false,
                py_to_value(ok, "value", PY_FUNC_BIN)?,
                py_from_value(ok, "raw.value")?,
            ),
        };
        Ok(Self {
            comments: py_comments(&sig.func_comments),
            snake: to_snake_case(&sig.func_name),
            kind_member: to_snake_case_upper(&sig.func_name),
            param_count,
            req_signature: signature.join(", "),
            req_encode_entries: encode_entries.join("\n"),
            req_decode_stmts: decode_stmts.join("\n"),
            req_decode_return,
            req_return_type,
            result_is_unit,
            res_ok_to_value,
            res_ok_from_value,
        })
    }
}
