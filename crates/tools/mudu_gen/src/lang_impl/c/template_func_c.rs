//! Askama template data for C MSSP func codecs.
//!
//! Renders the shared frame preamble ([`TemplateFuncHeaderC`]: the
//! `<prefix>_message_kind` enum plus the 16-byte header helpers) and one
//! block of request / result codec functions per WIT function
//! ([`TemplateFuncC`]). All payload statements use the explicit
//! `mp_writer`/`mp_reader` functions of the binding runtime via
//! [`crate::lang_impl::c::c_codec`]. Multi-parameter requests decode into a
//! generated holder struct; single-parameter requests decode into the bare
//! parameter type. Results decode into a per-function
//! `{ int is_err; <err> err; <ok> value; }` holder.

use crate::lang_impl::c::c_codec::{
    C_STYLE_FUNC, CMapEntry, CollectingCNamer, c_decode_stmts, c_encode_stmts, c_ident, c_join,
    c_map_decode_loop, c_type,
};
use crate::lang_impl::lang::func_info::{FuncResult, FuncSig, analyze_func};
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;
use askama::Template;
use mudu::common::result::RS;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case, to_snake_case_upper};
use mudu_binding::universal::uni_data_type::UniDataType;

/// A `<prefix>_message_kind` enum case pinned to a WIT function.
pub struct CKindInfo {
    /// Doc comment of the originating WIT function.
    pub comment: String,
    /// Enumerator constant (`UNI_SYSCALL_GET`).
    pub const_name: String,
    /// The pinned discriminant (1-based WIT declaration order).
    pub discriminant: u32,
}

/// The C symbol prefix of one WIT file's func surface (`uni_syscall`).
fn file_prefix(cfg: &CodegenCfg) -> String {
    if cfg.func_module_name.is_empty() {
        // Degenerate case (direct CodeGen call without a module name): keep
        // the generated symbols scoped rather than colliding with the mpack
        // runtime names.
        "mssp".to_string()
    } else {
        to_snake_case(&cfg.func_module_name)
    }
}

/// Askama template for the shared C MSSP frame preamble.
#[derive(Template)]
#[template(path = "c/func_header.h.jinja", escape = "none")]
pub struct TemplateFuncHeaderC {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Snake-case symbol prefix (`uni_syscall`).
    pub prefix: String,
    /// Upper-snake macro prefix (`UNI_SYSCALL`).
    pub macro_prefix: String,
    /// Message-kind cases, one per WIT function.
    pub kinds: Vec<CKindInfo>,
}

impl TemplateFuncHeaderC {
    /// Build the preamble template from all functions of one WIT file.
    pub fn from(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<Self> {
        let macro_prefix = if cfg.func_module_name.is_empty() {
            "MSSP".to_string()
        } else {
            to_snake_case_upper(&cfg.func_module_name)
        };
        let mut kinds = Vec::with_capacity(funcs.len());
        for func in &funcs {
            let sig = analyze_func(func)?;
            kinds.push(CKindInfo {
                comment: sig.func_comments.clone(),
                const_name: format!("{}_{}", macro_prefix, to_snake_case_upper(&sig.func_name)),
                discriminant: sig.func_index,
            });
        }
        Ok(Self {
            prefix: file_prefix(&cfg),
            macro_prefix,
            kinds,
            cfg,
        })
    }
}

/// A field of a generated request holder struct (funcs with > 1 parameter).
pub struct CHolderField {
    /// Rendered declaration line (`uni_oid oid;`).
    pub decl: String,
}

/// Askama template for one function's C MSSP codec functions.
#[derive(Template)]
#[template(path = "c/func.h.jinja", escape = "none")]
pub struct TemplateFuncC {
    #[allow(unused)]
    /// Generation configuration.
    pub cfg: CodegenCfg,
    /// Precomputed per-function rendering data.
    pub func: FuncRenderC,
}

/// Precomputed rendering data for one WIT function.
pub struct FuncRenderC {
    /// Doc comments of the function (`//` lines; empty when undocumented).
    pub comments: String,
    /// Snake-case symbol prefix (`uni_syscall`).
    pub prefix: String,
    /// Snake-case function name (`fs_open`).
    pub func_snake: String,
    /// Number of positional parameters.
    pub param_count: usize,
    /// Precomputed composite holder typedefs referenced by this function.
    pub holder_decls: Vec<String>,
    /// Request holder struct name when `param_count > 1`.
    pub req_holder_name: String,
    /// Request holder struct fields (when `param_count > 1`).
    pub req_holder_fields: Vec<CHolderField>,
    /// Request encoder parameter list (`const uni_oid *oid, mp_bin key`).
    pub req_signature: String,
    /// Request encode function body (indented).
    pub req_encode_body: String,
    /// Request decode output type: the holder name for `param_count > 1`,
    /// the bare parameter type for one parameter; empty for none.
    pub req_decode_out_ty: String,
    /// Request decode function body (indented).
    pub req_decode_body: String,
    /// Whether the result is a unit result (`result<_, E>`).
    pub result_is_unit: bool,
    /// Result holder struct name (`uni_syscall_get_result`).
    pub res_holder_name: String,
    /// Result ok-value member declaration (`uni_syscall_get_result_opt value;`); empty for unit results.
    pub res_value_decl: String,
    /// Wire error type name (`uni_error`).
    pub err_ty: String,
    /// Result encode function body (indented).
    pub res_encode_body: String,
    /// Result decode function body (indented).
    pub res_decode_body: String,
}

impl TemplateFuncC {
    /// Build the per-function template from a parsed WIT function.
    pub fn from(func: WitFuncDef, cfg: CodegenCfg) -> RS<Self> {
        let sig = analyze_func(&func)?;
        Ok(Self {
            func: FuncRenderC::from_sig(&sig, &cfg)?,
            cfg,
        })
    }
}

/// Whether a func parameter of this type is passed by const pointer (named
/// record/variant types) rather than by value (scalars, enums, slices,
/// wrappers, tuples). Unregistered identifiers default to by-pointer: the
/// generated `T_encode(w, &v)` shape works for every named type.
fn param_by_ref(ty: &UniDataType, cfg: &CodegenCfg) -> bool {
    match ty {
        UniDataType::Identifier(name) => !matches!(
            cfg.type_kinds
                .get(&to_pascal_case(name))
                .map(|k| k.as_str()),
            Some("enum")
        ),
        _ => false,
    }
}

impl FuncRenderC {
    fn from_sig(sig: &FuncSig, cfg: &CodegenCfg) -> RS<Self> {
        let prefix = file_prefix(cfg);
        let macro_prefix = if cfg.func_module_name.is_empty() {
            "MSSP".to_string()
        } else {
            to_snake_case_upper(&cfg.func_module_name)
        };
        // The function name only appears as a substring of the generated
        // symbols (`uni_syscall_delete_request_encode` is not a keyword even
        // though `delete` is), so it is not keyword-sanitized.
        let func_snake = to_snake_case(&sig.func_name);
        let kind_const = format!("{}_{}", macro_prefix, to_snake_case_upper(&sig.func_name));
        let mut namer = CollectingCNamer::new();
        let _ = LangKind::C;

        // ---- request side ----
        let mut signature = Vec::with_capacity(sig.params.len());
        let mut encode_lines = Vec::new();
        let mut map_entries = Vec::new();
        let mut holder_fields = Vec::new();
        let mut req_decode_out_ty = String::new();
        let multi = sig.params.len() > 1;
        let req_holder_name = format!("{}_{}_request", prefix, func_snake);
        for (index, param) in sig.params.iter().enumerate() {
            let param_name = c_ident(&to_snake_case(&param.param_name));
            let base = format!("{}_{}_{}", prefix, func_snake, param_name);
            namer.begin(base.clone());
            let ty = c_type(&param.param_type, &mut namer)?;
            let by_ref = param_by_ref(&param.param_type, cfg);
            if by_ref {
                signature.push(format!("const {} *{}", ty, param_name));
            } else {
                signature.push(format!("{} {}", ty, param_name));
            }
            let expr = if by_ref {
                format!("(*{})", param_name)
            } else {
                param_name.clone()
            };
            encode_lines.push(format!("    mpw_u64(w, {}u);", index + 1));
            c_encode_stmts(
                C_STYLE_FUNC,
                &param.param_type,
                &expr,
                4,
                0,
                &mut encode_lines,
            )?;
            let target = if multi {
                format!("out->{}", param_name)
            } else {
                "(*out)".to_string()
            };
            namer.begin(base);
            let mut dec = Vec::new();
            c_decode_stmts(
                C_STYLE_FUNC,
                &param.param_type,
                &target,
                0,
                0,
                &mut dec,
                &mut namer,
            )?;
            let dec = dec
                .iter()
                .map(|line| {
                    if line.is_empty() {
                        line.clone()
                    } else {
                        format!("                    {}", line)
                    }
                })
                .collect::<Vec<_>>();
            map_entries.push(CMapEntry {
                number: (index + 1) as u32,
                decode_stmts: c_join(&dec),
            });
            if multi {
                holder_fields.push(CHolderField {
                    decl: format!("    {} {};", ty, param_name),
                });
            } else {
                req_decode_out_ty = ty;
            }
        }
        let mut req_encode_body = vec![format!("    {}_write_header(w, {});", prefix, kind_const)];
        if !sig.params.is_empty() {
            req_encode_body.push(format!("    mpw_map_header(w, {}u);", sig.params.len()));
            req_encode_body.extend(encode_lines);
        }
        req_encode_body.push("    return mpw_ok(w) ? 0 : -1;".to_string());

        let mut req_decode_body = vec![
            format!("    {}_message_kind kind;", prefix),
            "    (void)a;".to_string(),
            format!("    if ({}_decode_header(r, &kind) != 0) {{", prefix),
            "        return -1;".to_string(),
            "    }".to_string(),
            format!("    if (kind != {}) {{", kind_const),
            "        mpr_fail(r);".to_string(),
            "        return -1;".to_string(),
            "    }".to_string(),
        ];
        if sig.params.is_empty() {
            req_decode_body.push("    if (!mpr_done(r)) {".to_string());
            req_decode_body.push("        mpr_fail(r);".to_string());
            req_decode_body.push("        return -1;".to_string());
            req_decode_body.push("    }".to_string());
            req_decode_body.push("    return 0;".to_string());
        } else {
            req_decode_body.push("    memset(out, 0, sizeof(*out));".to_string());
            req_decode_body.push(c_map_decode_loop(&map_entries, 4));
            req_decode_body.push("    if (!mpr_done(r)) {".to_string());
            req_decode_body.push("        mpr_fail(r);".to_string());
            req_decode_body.push("        return -1;".to_string());
            req_decode_body.push("    }".to_string());
            req_decode_body.push("    return mpr_ok(r) ? 0 : -1;".to_string());
        }
        if multi {
            req_decode_out_ty = req_holder_name.clone();
        }

        // ---- result side ----
        let err_ty_raw = match &sig.result {
            FuncResult::Unit { err } => err.clone(),
            FuncResult::Value { err, .. } => err.clone(),
        };
        namer.begin(format!("{}_{}_err", prefix, func_snake));
        let err_ty = c_type(&err_ty_raw, &mut namer)?;
        let res_holder_name = format!("{}_{}_result", prefix, func_snake);
        let (result_is_unit, res_value_decl, ok_encode_lines, ok_decode_lines) = match &sig.result {
            FuncResult::Unit { .. } => (true, String::new(), None, None),
            FuncResult::Value { ok, .. } => {
                namer.begin(format!("{}_{}_result", prefix, func_snake));
                let ok_ty = c_type(ok, &mut namer)?;
                let mut enc = Vec::new();
                c_encode_stmts(C_STYLE_FUNC, ok, "res->value", 8, 0, &mut enc)?;
                namer.begin(format!("{}_{}_result", prefix, func_snake));
                let mut dec = Vec::new();
                c_decode_stmts(C_STYLE_FUNC, ok, "out->value", 12, 0, &mut dec, &mut namer)?;
                (false, format!("    {} value;", ok_ty), Some(enc), Some(dec))
            }
        };
        let mut res_encode_body = vec![
            format!("    {}_write_header(w, {});", prefix, kind_const),
            "    mpw_array_header(w, 2u);".to_string(),
            "    if (res->is_err) {".to_string(),
            "        mpw_u64(w, 1u);".to_string(),
            format!("        {}_encode(w, &(res->err));", err_ty),
            "    } else {".to_string(),
            "        mpw_u64(w, 0u);".to_string(),
        ];
        match &ok_encode_lines {
            Some(lines) => res_encode_body.extend(lines.iter().cloned()),
            None => {
                // unit success: the `0u8` placeholder payload
                res_encode_body.push("        mpw_u64(w, 0u);".to_string())
            }
        }
        res_encode_body.push("    }".to_string());
        res_encode_body.push("    return mpw_ok(w) ? 0 : -1;".to_string());

        let mut res_decode_body = vec![
            format!("    {}_message_kind kind;", prefix),
            "    (void)a;".to_string(),
            format!("    if ({}_decode_header(r, &kind) != 0) {{", prefix),
            "        return -1;".to_string(),
            "    }".to_string(),
            format!("    if (kind != {}) {{", kind_const),
            "        mpr_fail(r);".to_string(),
            "        return -1;".to_string(),
            "    }".to_string(),
            "    memset(out, 0, sizeof(*out));".to_string(),
            "    if (mpr_array_header(r) != 2u) {".to_string(),
            "        mpr_fail(r);".to_string(),
            "        return -1;".to_string(),
            "    }".to_string(),
            "    {".to_string(),
            "        uint64_t tag = mpr_u64(r);".to_string(),
            "        if (tag == 0u) {".to_string(),
            "            out->is_err = 0;".to_string(),
        ];
        match &ok_decode_lines {
            Some(lines) => res_decode_body.extend(lines.iter().cloned()),
            None => {
                // consume the `0u8` placeholder payload
                res_decode_body.push("            (void)mpr_u64(r);".to_string())
            }
        }
        res_decode_body.extend([
            "        } else if (tag == 1u) {".to_string(),
            "            out->is_err = 1;".to_string(),
            format!("            {}_decode(r, a, &(out->err));", err_ty),
            "        } else {".to_string(),
            "            mpr_fail(r);".to_string(),
            "            return -1;".to_string(),
            "        }".to_string(),
            "    }".to_string(),
            "    if (!mpr_done(r)) {".to_string(),
            "        mpr_fail(r);".to_string(),
            "        return -1;".to_string(),
            "    }".to_string(),
            "    return mpr_ok(r) ? 0 : -1;".to_string(),
        ]);

        let holder_decls = namer.holders().iter().map(|h| h.decl()).collect();
        Ok(Self {
            comments: sig.func_comments.clone(),
            prefix,
            func_snake,
            param_count: sig.params.len(),
            holder_decls,
            req_holder_name,
            req_holder_fields: holder_fields,
            req_signature: signature.join(", "),
            req_encode_body: c_join(&req_encode_body),
            req_decode_out_ty,
            req_decode_body: c_join(&req_decode_body),
            result_is_unit,
            res_holder_name,
            res_value_decl,
            err_ty,
            res_encode_body: c_join(&res_encode_body),
            res_decode_body: c_join(&res_decode_body),
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::{TemplateFuncC, TemplateFuncHeaderC};
    use crate::src_gen::codegen_cfg::CodegenCfg;
    use crate::src_gen::wit_def::WitFuncDef;
    use askama::Template;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::RecordField;
    use mudu_binding::universal::uni_result_type::UniResultType;

    fn cfg() -> CodegenCfg {
        let mut cfg = CodegenCfg::new();
        cfg.with_func_codec = true;
        cfg.func_module_name = "UniSyscall".to_string();
        cfg
    }

    fn field(name: &str, ty: UniDataType) -> RecordField {
        RecordField::new(String::new(), name.to_string(), ty)
    }

    fn result_ty(ok: Option<UniDataType>) -> UniDataType {
        UniDataType::Result(UniResultType {
            ok: ok.map(Box::new),
            err: Some(Box::new(UniDataType::Identifier("uni-error".to_string()))),
        })
    }

    fn get_func() -> WitFuncDef {
        WitFuncDef {
            func_comments: String::new(),
            func_name: "get".to_string(),
            params: vec![
                field("oid", UniDataType::Identifier("uni-oid".to_string())),
                field("key", UniDataType::Binary),
            ],
            returns: vec![field(
                "",
                result_ty(Some(UniDataType::Option(Box::new(UniDataType::Binary)))),
            )],
            func_index: 6,
        }
    }

    #[test]
    fn func_header_renders_message_kinds_and_frame_helpers() -> RS<()> {
        let template = TemplateFuncHeaderC::from(vec![get_func()], cfg())?;
        let out = template.render().unwrap();
        assert!(out.contains("UNI_SYSCALL_GET = 6u,"));
        assert!(out.contains("} uni_syscall_message_kind;"));
        assert!(out.contains(
            "MP_INLINE void uni_syscall_write_header(mp_writer *w, uni_syscall_message_kind kind)"
        ));
        assert!(out.contains(
            "MP_INLINE int uni_syscall_decode_header(mp_reader *r, uni_syscall_message_kind *out)"
        ));
        assert!(out.contains("0x4d535350u"));
        Ok(())
    }

    #[test]
    fn func_renders_request_and_result_codecs() -> RS<()> {
        let template = TemplateFuncC::from(get_func(), cfg())?;
        let out = template.render().unwrap();
        // request holder (2 params)
        assert!(out.contains("} uni_syscall_get_request;"));
        assert!(out.contains("    uni_oid oid;"));
        assert!(out.contains("    mp_bin key;"));
        // option<list<u8>> result wrapper
        assert!(out.contains(
            "typedef struct { int has_value; mp_bin value; } uni_syscall_get_result_opt;"
        ));
        assert!(out.contains("} uni_syscall_get_result;"));
        assert!(out.contains("    uni_syscall_get_result_opt value;"));
        // request encode: header, map, bin param (FUNC style: bin, not array)
        assert!(out.contains("MP_INLINE int uni_syscall_get_request_encode(mp_writer *w, const uni_oid *oid, mp_bin key)"));
        assert!(out.contains("uni_syscall_write_header(w, UNI_SYSCALL_GET);"));
        assert!(out.contains("mpw_bin(w, (key).data, (key).len);"));
        assert!(out.contains("uni_oid_encode(w, &((*oid)));"));
        // result encode/decode
        assert!(out.contains("MP_INLINE int uni_syscall_get_result_encode(mp_writer *w, const uni_syscall_get_result *res)"));
        assert!(out.contains("uni_error_encode(w, &(res->err));"));
        assert!(out.contains("if ((res->value).has_value) {"));
        assert!(out.contains("uni_error_decode(r, a, &(out->err));"));
        assert!(out.contains("return mpr_ok(r) ? 0 : -1;"));
        Ok(())
    }

    #[test]
    fn unit_result_uses_placeholder_payload() -> RS<()> {
        let func = WitFuncDef {
            func_comments: String::new(),
            func_name: "close-session".to_string(),
            params: vec![field("oid", UniDataType::Identifier("uni-oid".to_string()))],
            returns: vec![field("", result_ty(None))],
            func_index: 5,
        };
        let template = TemplateFuncC::from(func, cfg())?;
        let out = template.render().unwrap();
        // single-param request: decodes into the bare parameter type
        assert!(out.contains(
            "uni_syscall_close_session_request_decode(mp_reader *r, mp_arena *a, uni_oid *out)"
        ));
        assert!(out.contains("uni_oid_decode(r, a, &((*out)));"));
        // unit result: no value member, placeholder payload
        assert!(!out.contains(" value;"));
        assert!(out.contains("mpw_u64(w, 0u);"));
        assert!(out.contains("(void)mpr_u64(r);"));
        Ok(())
    }

    #[test]
    fn tuple_list_params_get_holder_types() -> RS<()> {
        let func = WitFuncDef {
            func_comments: String::new(),
            func_name: "relation-get".to_string(),
            params: vec![
                field("oid", UniDataType::Identifier("uni-oid".to_string())),
                field(
                    "key",
                    UniDataType::Array(Box::new(UniDataType::Tuple(vec![
                        UniDataType::Scalar(mudu_binding::universal::uni_scalar::UniScalar::U64),
                        UniDataType::Binary,
                    ]))),
                ),
            ],
            returns: vec![field("", result_ty(None))],
            func_index: 21,
        };
        let template = TemplateFuncC::from(func, cfg())?;
        let out = template.render().unwrap();
        assert!(
            out.contains("    uint64_t f0;\n    mp_bin f1;\n} uni_syscall_relation_get_key_tuple;")
        );
        assert!(out.contains(
            "typedef struct { uni_syscall_relation_get_key_tuple *items; uint32_t len; } uni_syscall_relation_get_key_list;"
        ));
        assert!(out.contains("mpw_array_header(w, (key).len);"));
        assert!(out.contains("mpw_u64(w, (uint64_t)(((key).items[i0]).f0));"));
        Ok(())
    }
}
