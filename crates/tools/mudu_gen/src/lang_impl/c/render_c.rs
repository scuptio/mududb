//! C rendering back-end.
//!
//! Entity path (`mgen entity`): one self-contained freestanding header per
//! table over the guest `mudu_sys.h` layer.
//!
//! Message path (`mgen message`): one self-contained header per WIT file in
//! the `mududb` C binding layout — `static inline` MessagePack codecs over
//! the hand-written runtime (`mududb/codec/mpack.h`), with records as
//! structs, variants as tag-enum + tagged-union pairs, enums as C enums and
//! MSSP func surfaces as frame/request/result codec functions. Composite
//! value shapes (`list` slices, `option` wrappers, tuples) are named
//! per-position by the collecting namer in [`c_codec`].
//!
//! Cross-file reference discipline (what makes the headers acyclic and
//! self-contained): named record/variant types are referenced by pointer in
//! every position where a by-value dependency would be cyclic — variant
//! payloads and all `box<T>` shapes are arena-owned pointers decoded through
//! the generated `<T>_decode_new` — while record fields, enum payloads,
//! slice elements and func holders keep by-value (complete-type) references.
//! Each header forward-declares (guarded `typedef` + codec prototypes) the
//! record/variant types it defines and the ones it references by pointer
//! only; imports needed COMPLETE are included before the content
//! (`early_includes`), imports referenced only by pointer are included after
//! it (`late_includes`, providing the codec definitions the prototypes
//! announce).

use askama::Template;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case_upper};
use mudu_binding::table::table_def::TableDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::UniEnumDef;
use std::collections::HashMap;
use std::sync::Arc;

use crate::lang_impl::c::c_codec::{c_const_name, c_type_name};
use crate::lang_impl::c::template_entity_c::TemplateEntityC;
use crate::lang_impl::c::template_enum_c::TemplateEnumC;
use crate::lang_impl::c::template_file_c::{CFileInfo, TemplateFileC};
use crate::lang_impl::c::template_func_c::{TemplateFuncC, TemplateFuncHeaderC};
use crate::lang_impl::c::template_record_c::{TemplateRecordC, TemplateRecordCodecC};
use crate::lang_impl::c::template_variant_c::{TemplateVariantC, TemplateVariantCodecC};
use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::render::Render;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::wit_def::WitFuncDef;

/// Create a C renderer.
pub fn create_render() -> Arc<dyn Render> {
    Arc::new(RenderC::new())
}

struct RenderC {}

impl Render for RenderC {
    fn render(&self, template: AbstractTemplate) -> RS<String> {
        // An entity header is self-contained (it includes the guest facade
        // directly), so it bypasses the message file wrapper.
        if let [TemplateKind::Entity(entity)] = template.elements.as_slice() {
            return Self::render_entity_c(entity.clone());
        }
        let guard = format!(
            "MUDUD_TYPES_{}_H",
            to_snake_case_upper(&guard_base(&template.elements))
        );
        let type_kinds = elements_type_kinds(&template.elements);
        let complete_refs = complete_references(&template.elements, &type_kinds);
        // Each WIT `use a:b/c-d/c-d` item maps to an include of the
        // generated header named after the type (PascalCase of the last
        // segment): before the content when a complete type is needed,
        // after it when only codec prototypes are.
        let mut includes = Vec::new();
        let mut late_includes = Vec::new();
        let mut late_types = Vec::new();
        for path in &template.using_stmts {
            if let Some(name) = path.last() {
                let pascal = to_pascal_case(name);
                if complete_refs.contains(&pascal) {
                    includes.push(pascal);
                } else {
                    late_includes.push(pascal.clone());
                    if !matches!(type_kinds.get(&pascal).map(|k| k.as_str()), Some("enum")) {
                        late_types.push(pascal);
                    }
                }
            }
        }
        let blocks = self.render_inner(&type_kinds, &late_types, template.elements)?;
        let template_file = TemplateFileC {
            file: CFileInfo {
                guard,
                includes,
                late_includes,
                blocks,
            },
        };
        let s = template_file
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Internal, "render error", e))?;
        Ok(s)
    }
}

/// The shared type-kind registry (PascalCase name → enum/variant/record)
/// taken from the elements' generation config.
fn elements_type_kinds(elements: &[TemplateKind]) -> HashMap<String, String> {
    for element in elements {
        let cfg = match element {
            TemplateKind::Enum((_, cfg))
            | TemplateKind::Variant((_, cfg))
            | TemplateKind::Record((_, cfg))
            | TemplateKind::Table((_, cfg))
            | TemplateKind::FuncHeader((_, cfg))
            | TemplateKind::Func((_, cfg)) => cfg,
            TemplateKind::Entity(_) => continue,
        };
        return cfg.type_kinds.clone();
    }
    HashMap::new()
}

/// Derive the include-guard base of one generated header: the WIT file stem
/// carried in `func_module_name` when available (directory-mode `mgen
/// message` sets it for every file), otherwise the first type name in the
/// file.
fn guard_base(elements: &[TemplateKind]) -> String {
    for element in elements {
        let cfg = match element {
            TemplateKind::Enum((_, cfg))
            | TemplateKind::Variant((_, cfg))
            | TemplateKind::Record((_, cfg))
            | TemplateKind::Table((_, cfg))
            | TemplateKind::FuncHeader((_, cfg))
            | TemplateKind::Func((_, cfg)) => cfg,
            TemplateKind::Entity(_) => continue,
        };
        if !cfg.func_module_name.is_empty() {
            return cfg.func_module_name.clone();
        }
    }
    for element in elements {
        match element {
            TemplateKind::Enum((def, _)) => return to_pascal_case(&def.enum_name),
            TemplateKind::Variant((def, _)) => return to_pascal_case(&def.variant_name),
            TemplateKind::Record((def, _)) => return to_pascal_case(&def.record_name),
            _ => continue,
        }
    }
    "Unnamed".to_string()
}

/// The name a block defines (PascalCase of the WIT type name), for the
/// intra-file dependency analysis. Func/table blocks define no types.
fn block_type_name(element: &TemplateKind) -> Option<String> {
    match element {
        TemplateKind::Enum((def, _)) => Some(to_pascal_case(&def.enum_name)),
        TemplateKind::Variant((def, _)) => Some(to_pascal_case(&def.variant_name)),
        TemplateKind::Record((def, _)) => Some(to_pascal_case(&def.record_name)),
        _ => None,
    }
}

/// Collect the identifiers `ty` references where the C representation needs
/// the named type COMPLETE (driving the early includes): by-value named
/// members, and slice elements (the decode subscripts element pointers).
/// `box` positions are pointers and never need completeness.
fn complete_idents(ty: &UniDataType, out: &mut Vec<String>) {
    match ty {
        UniDataType::Identifier(name) => out.push(name.clone()),
        UniDataType::Array(inner) => complete_idents(inner, out),
        UniDataType::Option(inner) => complete_idents(inner, out),
        UniDataType::Tuple(elems) => {
            for elem in elems {
                complete_idents(elem, out);
            }
        }
        UniDataType::Result(result_ty) => {
            if let Some(ok) = &result_ty.ok {
                complete_idents(ok, out);
            }
            if let Some(err) = &result_ty.err {
                complete_idents(err, out);
            }
        }
        _ => {}
    }
}

/// Collect the PascalCase names of the types this file references in a
/// complete-type position (driving the early includes).
fn complete_references(
    elements: &[TemplateKind],
    type_kinds: &HashMap<String, String>,
) -> Vec<String> {
    let mut idents = Vec::new();
    for element in elements {
        match element {
            TemplateKind::Record((def, _)) => {
                for field in &def.record_fields {
                    complete_idents(&field.rf_type, &mut idents);
                }
            }
            TemplateKind::Variant((def, _)) => {
                for case in &def.variant_cases {
                    if let Some(ty) = &case.vc_case_type {
                        // Variant payloads of named record/variant types are
                        // pointer members (no completeness needed); enum
                        // payloads and every other shape keep by-value
                        // references.
                        let by_pointer = matches!(ty, UniDataType::Identifier(name)
                            if !matches!(type_kinds.get(&to_pascal_case(name)).map(|k| k.as_str()), Some("enum")));
                        if !by_pointer {
                            complete_idents(ty, &mut idents);
                        }
                    }
                }
            }
            TemplateKind::Func((func, _)) => {
                for param in &func.params {
                    complete_idents(&param.rf_type, &mut idents);
                }
                for ret in &func.returns {
                    complete_idents(&ret.rf_type, &mut idents);
                }
            }
            _ => {}
        }
    }
    idents
        .into_iter()
        .map(|name| to_pascal_case(&name))
        .collect()
}

/// Collect the identifiers `ty` uses in a BY-VALUE position for the
/// intra-file struct ordering (through `option`/`tuple` only; `list`
/// slices and `box` are pointer positions and contribute no struct
/// dependency).
fn by_value_idents(ty: &UniDataType, out: &mut Vec<String>) {
    match ty {
        UniDataType::Identifier(name) => out.push(name.clone()),
        UniDataType::Option(inner) => by_value_idents(inner, out),
        UniDataType::Tuple(elems) => {
            for elem in elems {
                by_value_idents(elem, out);
            }
        }
        _ => {}
    }
}

/// The intra-file by-value dependencies of one block: PascalCase names of
/// types defined in the same file whose definition must be complete before
/// this block's struct. Variant payloads of named record/variant types are
/// pointer members and contribute no dependency; enum payloads do.
fn block_by_value_deps(
    element: &TemplateKind,
    type_kinds: &HashMap<String, String>,
) -> Vec<String> {
    let mut idents = Vec::new();
    match element {
        TemplateKind::Record((def, _)) => {
            for field in &def.record_fields {
                by_value_idents(&field.rf_type, &mut idents);
            }
        }
        TemplateKind::Variant((def, _)) => {
            for case in &def.variant_cases {
                if let Some(ty) = &case.vc_case_type {
                    let by_pointer = matches!(ty, UniDataType::Identifier(name)
                        if !matches!(type_kinds.get(&to_pascal_case(name)).map(|k| k.as_str()), Some("enum")));
                    if !by_pointer {
                        by_value_idents(ty, &mut idents);
                    }
                }
            }
        }
        _ => {}
    }
    idents
        .into_iter()
        .map(|name| to_pascal_case(&name))
        .collect()
}

/// Order type blocks so every intra-file by-value dependency is defined
/// before its user (stable: WIT declaration order is kept when there is no
/// dependency). Pointer references (list/box) and codec calls jump freely
/// through the file-level forward declarations.
fn topo_order(
    blocks: &[(String, TemplateKind)],
    type_kinds: &HashMap<String, String>,
) -> RS<Vec<usize>> {
    let names: Vec<&str> = blocks.iter().map(|(name, _)| name.as_str()).collect();
    let deps: Vec<Vec<usize>> = blocks
        .iter()
        .map(|(_, element)| {
            block_by_value_deps(element, type_kinds)
                .iter()
                .filter_map(|dep| names.iter().position(|n| *n == dep))
                .collect()
        })
        .collect();
    let mut emitted = vec![false; blocks.len()];
    let mut order = Vec::with_capacity(blocks.len());
    loop {
        let mut progressed = false;
        for i in 0..blocks.len() {
            if !emitted[i] && deps[i].iter().all(|d| emitted[*d]) {
                emitted[i] = true;
                order.push(i);
                progressed = true;
            }
        }
        if order.len() == blocks.len() {
            return Ok(order);
        }
        if !progressed {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "cyclic by-value type containment is not representable in C"
            ));
        }
    }
}

/// Render one guarded forward-declaration block for a record/variant type:
/// the struct typedef plus the codec prototypes, so pointer references and
/// codec calls may jump forward. The per-type guard macro dedupes the block
/// when several headers in one translation unit forward-declare the same
/// type.
fn forward_decl(name: &str) -> String {
    let macro_name = format!("MUDUD_FWD_{}", c_const_name(name));
    format!(
        "#ifndef {macro_name}\n\
         #define {macro_name}\n\
         typedef struct {name} {name};\n\
         MP_INLINE void {name}_encode(mp_writer *w, const {name} *value);\n\
         MP_INLINE int {name}_decode(mp_reader *r, mp_arena *a, {name} *out);\n\
         MP_INLINE {name} *{name}_decode_new(mp_reader *r, mp_arena *a);\n\
         #endif /* {macro_name} */",
        name = c_type_name(name),
        macro_name = macro_name,
    )
}

/// Render the forward-declaration section: one guarded block per
/// record/variant defined in this file, plus one per record/variant
/// imported through a pointer-only reference (the late includes).
fn forward_section(
    blocks: &[(String, TemplateKind)],
    order: &[usize],
    late_types: &[String],
) -> Option<String> {
    let mut decls = Vec::new();
    for &i in order {
        let element = &blocks[i].1;
        let name = match element {
            TemplateKind::Record((def, _)) => def.record_name.clone(),
            TemplateKind::Variant((def, _)) => def.variant_name.clone(),
            _ => continue,
        };
        decls.push(forward_decl(&name));
    }
    for name in late_types {
        decls.push(forward_decl(name));
    }
    if decls.is_empty() {
        None
    } else {
        Some(decls.join("\n"))
    }
}

/// A built record/variant template kept alive for the codec pass.
enum TypeTemplate {
    /// Record template data (struct part already rendered).
    Record(TemplateRecordC),
    /// Variant template data (struct part already rendered).
    Variant(TemplateVariantC),
}

impl RenderC {
    fn new() -> Self {
        Self {}
    }

    fn render_inner(
        &self,
        type_kinds: &HashMap<String, String>,
        late_types: &[String],
        elements: Vec<TemplateKind>,
    ) -> RS<Vec<String>> {
        // Split type-defining blocks from func blocks. Layout: forward
        // declarations, then struct definitions (topologically ordered;
        // enums render whole), then the record/variant codec functions
        // (same order), then func blocks in WIT order. The two-phase type
        // layout is what lets mutually recursive types (uni-data-value) and
        // pointer references between blocks compile.
        let mut type_blocks: Vec<(String, TemplateKind)> = Vec::new();
        let mut func_blocks: Vec<TemplateKind> = Vec::new();
        for element in elements {
            match block_type_name(&element) {
                Some(name) => type_blocks.push((name, element)),
                None => func_blocks.push(element),
            }
        }
        let order = topo_order(&type_blocks, type_kinds)?;
        let mut code_blocks = Vec::with_capacity(type_blocks.len() * 2 + func_blocks.len() + 1);
        if let Some(forward) = forward_section(&type_blocks, &order, late_types) {
            code_blocks.push(forward);
        }
        let mut cells: Vec<Option<TemplateKind>> =
            type_blocks.into_iter().map(|(_, e)| Some(e)).collect();
        let mut built: Vec<Option<TypeTemplate>> = Vec::with_capacity(order.len());
        for &i in &order {
            let element = cells[i]
                .take()
                .ok_or_else(|| mudu_error!(ErrorCode::Internal, "duplicate topo index"))?;
            match element {
                TemplateKind::Enum((def, cfg)) => {
                    code_blocks.push(Self::render_enum_c(def, cfg)?);
                    built.push(None);
                }
                TemplateKind::Variant((def, cfg)) => {
                    let template = TemplateVariantC::from(def, cfg)?;
                    let s = template.render().map_err(|e| {
                        mudu_error!(ErrorCode::Decode, "render c variant template error", e)
                    })?;
                    code_blocks.push(s);
                    built.push(Some(TypeTemplate::Variant(template)));
                }
                TemplateKind::Record((def, cfg)) => {
                    let template = TemplateRecordC::from(def, cfg)?;
                    let s = template.render().map_err(|e| {
                        mudu_error!(ErrorCode::Decode, "render c record template error", e)
                    })?;
                    code_blocks.push(s);
                    built.push(Some(TypeTemplate::Record(template)));
                }
                _ => {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        "non-type template element in a C type block"
                    ));
                }
            }
        }
        for template in built.iter().flatten() {
            let s = match template {
                TypeTemplate::Variant(t) => {
                    let codec = TemplateVariantCodecC { data: t };
                    codec.render().map_err(|e| {
                        mudu_error!(
                            ErrorCode::Decode,
                            "render c variant codec template error",
                            e
                        )
                    })?
                }
                TypeTemplate::Record(t) => {
                    let codec = TemplateRecordCodecC { data: t };
                    codec.render().map_err(|e| {
                        mudu_error!(ErrorCode::Decode, "render c record codec template error", e)
                    })?
                }
            };
            code_blocks.push(s);
        }
        for element in func_blocks {
            code_blocks.push(Self::render_func_block(element)?);
        }
        Ok(code_blocks)
    }

    fn render_func_block(element: TemplateKind) -> RS<String> {
        match element {
            TemplateKind::Entity(_) => Err(mudu_error!(
                ErrorCode::Internal,
                "entity template mixed into a C message file"
            )),
            TemplateKind::Table(_) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                "table code generation for C is not implemented"
            )),
            TemplateKind::FuncHeader((funcs, cfg)) => Self::render_func_header_c(funcs, cfg),
            TemplateKind::Func((func, cfg)) => Self::render_func_c(func, cfg),
            _ => Err(mudu_error!(
                ErrorCode::Internal,
                "type template element in a C func block"
            )),
        }
    }

    fn render_enum_c(def: UniEnumDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateEnumC::from(def, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render c enum template error", e))?;
        Ok(s)
    }

    fn render_entity_c(table_schema: TableDef) -> RS<String> {
        let template = TemplateEntityC::from_table_schema(&table_schema)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render c entity template error", e))?;
        Ok(s)
    }

    fn render_func_header_c(funcs: Vec<WitFuncDef>, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncHeaderC::from(funcs, cfg)?;
        let s = template.render().map_err(|e| {
            mudu_error!(ErrorCode::Decode, "render c func header template error", e)
        })?;
        Ok(s)
    }

    fn render_func_c(func: WitFuncDef, cfg: CodegenCfg) -> RS<String> {
        let template = TemplateFuncC::from(func, cfg)?;
        let s = template
            .render()
            .map_err(|e| mudu_error!(ErrorCode::Decode, "render c func template error", e))?;
        Ok(s)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::lang_impl::c::lang_def::create_render_c;
    use crate::lang_impl::lang::abstract_template::AbstractTemplate;
    use crate::lang_impl::lang::template_kind::TemplateKind;
    use crate::src_gen::code_gen::CodeGen;
    use crate::src_gen::codegen_cfg::CodegenCfg;
    use crate::src_gen::gen_message::gen_message_with_cfg;
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_def::{RecordField, UniRecordDef};
    use mudu_binding::universal::uni_scalar::UniScalar;

    /// A record-only WIT file renders as a guarded self-contained header.
    #[test]
    fn render_wraps_message_blocks_in_a_file() -> RS<()> {
        let def = UniRecordDef {
            record_comments: String::new(),
            record_name: "uni-oid".to_string(),
            record_fields: vec![RecordField::new(
                String::new(),
                "h".to_string(),
                UniDataType::Scalar(UniScalar::U64),
            )],
        };
        let mut cfg = CodegenCfg::new();
        cfg.func_module_name = "UniOid".to_string();
        let mut template = AbstractTemplate::new();
        template.elements.push(TemplateKind::Record((def, cfg)));
        let out = create_render_c().render(template)?;
        assert!(out.contains("#ifndef MUDUD_TYPES_UNI_OID_H"));
        assert!(out.contains("#define MUDUD_TYPES_UNI_OID_H"));
        assert!(out.contains("#include \"mududb/codec/mpack.h\""));
        assert!(out.contains("#ifdef __cplusplus"));
        assert!(out.contains("struct uni_oid {"));
        assert!(out.contains("typedef struct uni_oid uni_oid;"));
        assert!(out.contains("#endif /* MUDUD_TYPES_UNI_OID_H */"));
        Ok(())
    }

    /// The full message pipeline: WIT text -> C header, records and MSSP
    /// func codecs included (mirrors the simple_func fixture used by the
    /// other languages' golden tests).
    #[test]
    fn generate_message_code_from_wit_produces_c() -> RS<()> {
        let wit = r#"
package mududb:api;

world host {
    import binding;
}

interface binding {
    record mu-oid {
        h: u64,
        l: u64,
    }

    record mu-error {
        err-code: u32,
        err-msg: string,
        err-details: list<u8>,
    }

    get: func(oid: mu-oid, key: list<u8>) -> result<option<list<u8>>, mu-error>;
    put: func(oid: mu-oid, value: list<u8>) -> result<_, mu-error>;
}
"#;
        let mut cfg = CodegenCfg::new();
        cfg.with_func_codec = true;
        cfg.func_module_name = "MuBinding".to_string();
        let out = CodeGen::generate_message_code_from_wit_with_cfg(wit, "c", None, cfg)?;
        assert!(out.contains("#ifndef MUDUD_TYPES_MU_BINDING_H"));
        assert!(out.contains("struct mu_oid {"));
        assert!(out.contains("struct mu_error {"));
        assert!(out.contains("mu_error err;"));
        assert!(out.contains("} mu_binding_message_kind;"));
        assert!(out.contains("MU_BINDING_GET = 1u,"));
        assert!(out.contains("MU_BINDING_PUT = 2u,"));
        assert!(out.contains("MP_INLINE int mu_binding_get_request_encode(mp_writer *w, const mu_oid *oid, mp_bin key)"));
        assert!(out.contains("MP_INLINE int mu_binding_put_result_decode(mp_reader *r, mp_arena *a, mu_binding_put_result *out)"));
        // func-context `list<u8>` is a MessagePack bin, record-context an array
        assert!(out.contains("mpw_bin(w, (key).data, (key).len);"));
        assert!(out.contains("mpw_array_header(w, (value->err_details).len);"));
        Ok(())
    }

    /// Cross-file references surface as includes of the generated headers.
    #[test]
    fn using_stmts_become_includes() -> RS<()> {
        let wit = r#"
package mududb:api;

world host {
    import binding;
}

use crate:universal:uni-oid/uni-oid;

interface binding {
    record mu-fs-stat {
        oid: uni-oid,
        length: u64,
    }
}
"#;
        let mut cfg = CodegenCfg::new();
        cfg.func_module_name = "MuFsStat".to_string();
        let out = CodeGen::generate_message_code_from_wit_with_cfg(wit, "c", None, cfg)?;
        assert!(out.contains("#include \"mududb/types/UniOid.h\""));
        assert!(out.contains("uni_oid oid;"));
        Ok(())
    }

    /// Pointer-only imports are forward-declared and included late, after
    /// the content (the include order that keeps cyclic WIT reference
    /// graphs — `uni-result-type` vs `uni-data-type` — compilable).
    #[test]
    fn pointer_only_imports_are_forward_declared_and_included_late() -> RS<()> {
        let wit = r#"
package mududb:api;

world host {
    import binding;
}

use crate:universal:uni-data-type/uni-data-type;

interface binding {
    record mu-result-type {
        ok: option<box<uni-data-type>>,
        err: option<box<uni-data-type>>,
    }
}
"#;
        let mut cfg = CodegenCfg::new();
        cfg.func_module_name = "MuResultType".to_string();
        cfg.type_kinds
            .insert("UniDataType".to_string(), "variant".to_string());
        let out = CodeGen::generate_message_code_from_wit_with_cfg(wit, "c", None, cfg)?;
        // forward declaration with a per-type guard macro
        assert!(out.contains("#ifndef MUDUD_FWD_UNI_DATA_TYPE"));
        assert!(out.contains("typedef struct uni_data_type uni_data_type;"));
        assert!(out.contains(
            "MP_INLINE uni_data_type *uni_data_type_decode_new(mp_reader *r, mp_arena *a);"
        ));
        // the include comes AFTER the content (late include)
        let content_at = out.find("struct mu_result_type {").unwrap();
        let include_at = out.find("#include \"mududb/types/UniDataType.h\"").unwrap();
        assert!(content_at < include_at);
        // the box decodes through decode_new (no sizeof of the pointee here)
        assert!(out.contains("uni_data_type *p0 = uni_data_type_decode_new(r, a);"));
        assert!(!out.contains("sizeof(uni_data_type)"));
        Ok(())
    }

    /// Mutually recursive types (a record holding a variant by value while
    /// the variant lists the record) render with forward declarations and
    /// the by-value dependency ordered first — the `uni-data-value` shape.
    #[test]
    fn mutually_recursive_types_get_forward_decls_and_topo_order() -> RS<()> {
        let wit = r#"
package mududb:api;

world host {
    import binding;
}

interface binding {
    record mu-data-value-field {
        field-name: string,
        field-value: mu-data-value,
    }

    variant mu-data-value {
        scalar(u64),
        record(list<mu-data-value-field>),
    }
}
"#;
        let mut cfg = CodegenCfg::new();
        cfg.func_module_name = "MuDataValue".to_string();
        let out = CodeGen::generate_message_code_from_wit_with_cfg(wit, "c", None, cfg)?;
        // forward typedefs + codec prototypes for both types
        assert!(out.contains("typedef struct mu_data_value mu_data_value;"));
        assert!(out.contains("typedef struct mu_data_value_field mu_data_value_field;"));
        assert!(out.contains(
            "MP_INLINE void mu_data_value_field_encode(mp_writer *w, const mu_data_value_field *value);"
        ));
        // the variant (by-value dependency of the record) is defined first
        let variant_at = out.find("struct mu_data_value {").unwrap();
        let record_at = out.find("struct mu_data_value_field {").unwrap();
        assert!(variant_at < record_at);
        // the slice holder may reference the record by pointer before its
        // definition (forward typedef covers it)
        assert!(out.contains(
            "typedef struct { mu_data_value_field *items; uint32_t len; } mu_data_value_record_list;"
        ));
        Ok(())
    }

    /// Generation is deterministic (byte-stable output for CI regen gates).
    #[test]
    fn generation_is_byte_stable() -> RS<()> {
        let wit_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../common/mudu_binding/wit");
        if !wit_dir.is_dir() {
            // Out-of-tree build (docs, packaging): skip rather than fail.
            return Ok(());
        }
        let out_a = mudu_sys::env_var::temp_dir().join("mudu_gen_c_stable_a");
        let out_b = mudu_sys::env_var::temp_dir().join("mudu_gen_c_stable_b");
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&out_a);
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&out_b);
        let mut cfg = CodegenCfg::new();
        cfg.with_func_codec = true;
        gen_message_with_cfg(&wit_dir, &out_a, "c".to_string(), None, cfg.clone(), None)?;
        gen_message_with_cfg(&wit_dir, &out_b, "c".to_string(), None, cfg, None)?;
        let entries = mudu_sys::fs::sync::sync_read_dir_entries(&out_a)?;
        assert_eq!(entries.len(), 23);
        for entry in entries {
            let a = mudu_sys::fs::sync::sync_read_to_string(entry.path())?;
            let b = mudu_sys::fs::sync::sync_read_to_string(out_b.join(entry.file_name()))?;
            assert_eq!(a, b, "byte-stable regen failed for {:?}", entry.file_name());
        }
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&out_a);
        let _ = mudu_sys::fs::sync::sync_remove_dir_all(&out_b);
        Ok(())
    }
}
