#![allow(clippy::unwrap_used)]

use super::TemplateTableCS;
use crate::src_gen::codegen_cfg::CodegenCfg;
use askama::Template;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_def::{RecordField, UniTableDef};
use mudu_binding::universal::uni_scalar::UniScalar;

fn sample_table_def() -> UniTableDef {
    UniTableDef {
        table_comments: "/// A user table.".to_string(),
        table_name: "user-account".to_string(),
        table_key: vec![RecordField {
            rf_comments: "/// Primary key.".to_string(),
            rf_name: "id".to_string(),
            rf_type: UniDatType::Scalar(UniScalar::I64),
        }],
        table_value: vec![
            RecordField {
                rf_comments: "/// User name.".to_string(),
                rf_name: "name".to_string(),
                rf_type: UniDatType::Scalar(UniScalar::String),
            },
            RecordField {
                rf_comments: String::new(),
                rf_name: "age".to_string(),
                rf_type: UniDatType::Scalar(UniScalar::I32),
            },
        ],
    }
}

#[test]
fn renders_table_struct_with_key_and_value() {
    let table = TemplateTableCS::from(sample_table_def(), CodegenCfg::new()).unwrap();
    let rendered = table.render().unwrap();

    assert!(rendered.contains("public struct UserAccount"));
    assert!(rendered.contains("public struct UserAccountKey"));
    assert!(rendered.contains("public struct UserAccountValue"));
    assert!(rendered.contains(
        "public required List<KeyValuePair<UserAccountKey, UserAccountValue>> ResultSet"
    ));
}

#[test]
fn renders_comments_when_present() {
    let mut table = TemplateTableCS::from(sample_table_def(), CodegenCfg::new()).unwrap();
    // TableInfo currently discards the top-level comment, so populate it directly
    // to exercise the template branch that emits it.
    table.table.table_comments = "/// A user table.".to_string();
    let rendered = table.render().unwrap();

    assert!(rendered.contains("/// A user table."));
    assert!(rendered.contains("/// Primary key."));
    assert!(rendered.contains("/// User name."));
}

#[test]
fn renders_default_constructor_when_enabled() {
    let mut cfg = CodegenCfg::new();
    cfg.impl_default = true;

    let table = TemplateTableCS::from(sample_table_def(), cfg).unwrap();
    let rendered = table.render().unwrap();

    assert!(rendered.contains("public UserAccountKey()"));
    assert!(rendered.contains("public UserAccountValue()"));
    assert!(rendered.contains("public UserAccount()"));
    assert!(rendered.contains("ResultSet = [];"));
    assert!(rendered.contains("Name = string.Empty;"));
    assert!(rendered.contains("Age = 0;"));
}

#[test]
fn omits_default_constructor_when_disabled() {
    let mut cfg = CodegenCfg::new();
    cfg.impl_default = false;

    let table = TemplateTableCS::from(sample_table_def(), cfg).unwrap();
    let rendered = table.render().unwrap();

    assert!(!rendered.contains("public UserAccountKey()"));
    assert!(!rendered.contains("public UserAccountValue()"));
    assert!(!rendered.contains("public UserAccount()"));
}

#[test]
fn marks_reference_types_as_required() {
    let table = TemplateTableCS::from(sample_table_def(), CodegenCfg::new()).unwrap();
    let rendered = table.render().unwrap();

    assert!(rendered.contains("public required string Name { get; set; }"));
    assert!(rendered.contains("public int Age { get; set; }"));
}
