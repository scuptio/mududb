//! Render every `TemplateKind` for the AssemblyScript and Python backends:
//! the per-kind arms of `render_inner` are only exercised through
//! `Render::render`.
#![allow(clippy::unwrap_used)]

use crate::lang_impl::lang::abstract_template::AbstractTemplate;
use crate::lang_impl::lang::lang_kind::LangKind;
use crate::lang_impl::lang::template_kind::TemplateKind;
use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::create_render::create_render;
use crate::src_gen::wit_def::WitFuncDef;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_def::{
    EnumCase, RecordField, UniEnumDef, UniRecordDef, UniTableDef, UniVariantDef, VariantCase,
};
use mudu_binding::universal::uni_scalar::UniScalar;

fn field(name: &str) -> RecordField {
    RecordField {
        rf_comments: String::new(),
        rf_name: name.to_string(),
        rf_type: UniDataType::Scalar(UniScalar::I32),
        rf_number: 1,
    }
}

fn sample_func() -> WitFuncDef {
    // MSSP func codecs only generate for result<T, E> returns.
    let result_type =
        UniDataType::Result(mudu_binding::universal::uni_result_type::UniResultType {
            ok: Some(Box::new(UniDataType::Scalar(UniScalar::I32))),
            err: Some(Box::new(UniDataType::Scalar(UniScalar::String))),
        });
    WitFuncDef {
        func_comments: String::new(),
        func_name: "get-item".to_string(),
        params: vec![field("id")],
        returns: vec![RecordField {
            rf_type: result_type,
            ..field("value")
        }],
        func_index: 1,
    }
}

fn render_all_kinds(lang: LangKind) -> String {
    let mut template = AbstractTemplate::new();
    template.namespace = "demo".to_string();
    let cfg = CodegenCfg::default();
    template.elements = vec![
        TemplateKind::Enum((
            UniEnumDef {
                enum_comments: String::new(),
                enum_name: "Status".to_string(),
                enum_cases: vec![
                    EnumCase {
                        ec_comments: String::new(),
                        ec_name: "Ok".to_string(),
                        ec_number: 0,
                    },
                    EnumCase {
                        ec_comments: String::new(),
                        ec_name: "Failed".to_string(),
                        ec_number: 1,
                    },
                ],
            },
            cfg.clone(),
        )),
        TemplateKind::Variant((
            UniVariantDef {
                variant_comments: String::new(),
                variant_name: "Outcome".to_string(),
                variant_cases: vec![
                    VariantCase {
                        vc_comments: String::new(),
                        vc_case_name: "hit".to_string(),
                        vc_case_type: Some(UniDataType::Scalar(UniScalar::String)),
                    },
                    VariantCase {
                        vc_comments: String::new(),
                        vc_case_name: "miss".to_string(),
                        vc_case_type: None,
                    },
                ],
            },
            cfg.clone(),
        )),
        TemplateKind::Record((
            UniRecordDef {
                record_comments: String::new(),
                record_name: "Item".to_string(),
                record_fields: vec![field("id"), field("name")],
            },
            cfg.clone(),
        )),
        TemplateKind::FuncHeader((vec![sample_func()], cfg.clone())),
        TemplateKind::Func((sample_func(), cfg.clone())),
    ];
    let render = create_render(&lang);
    render.render(template).unwrap()
}

#[test]
fn assemblyscript_renders_every_template_kind() {
    let out = render_all_kinds(LangKind::AssemblyScript);
    assert!(out.contains("Status"), "{out}");
    assert!(out.contains("Item"), "{out}");
    assert!(out.contains("Outcome"), "{out}");
    assert!(out.contains("GetItem"), "{out}");
}

#[test]
fn python_renders_every_template_kind() {
    let out = render_all_kinds(LangKind::Python);
    assert!(out.contains("Status"), "{out}");
    assert!(out.contains("Item"), "{out}");
}

#[test]
fn assemblyscript_rejects_table_elements() {
    // The AS backend has no table-template renderer: it must fail cleanly.
    let mut template = AbstractTemplate::new();
    template.elements = vec![TemplateKind::Table((
        UniTableDef {
            table_comments: String::new(),
            table_name: "t".to_string(),
            table_key: vec![],
            table_value: vec![],
        },
        CodegenCfg::default(),
    ))];
    let render = create_render(&LangKind::AssemblyScript);
    assert!(render.render(template).is_err());
}
