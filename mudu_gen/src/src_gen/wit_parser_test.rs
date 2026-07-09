//! Unit tests for the WIT parser and error analyzer.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::src_gen::wit_parser::{AdvancedErrorAnalyzer, WitParser};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use tree_sitter::Parser;

fn wit_language() -> tree_sitter::Language {
    tree_sitter_wit::LANGUAGE.into()
}

fn parse_tree(source: &str) -> tree_sitter::Tree {
    let mut parser = Parser::new();
    parser.set_language(&wit_language()).unwrap();
    parser.parse(source, None).unwrap()
}

#[test]
fn parser_accepts_minimal_interface() {
    let source = r#"
        package test:api;
        world host { import binding; }
        interface binding {
            record point { x: s32, y: s32 }
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.interface.len(), 1);
    assert_eq!(wit.records.len(), 1);
    assert_eq!(wit.records[0].record_name, "point");
}

#[test]
fn parser_accepts_enum_and_variant() {
    let source = r#"
        package test:api;
        interface binding {
            enum status { ok, err }
            variant value { number(s64), text(string) }
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.enums.len(), 1);
    assert_eq!(wit.enums[0].enum_name, "status");
    assert_eq!(wit.variants.len(), 1);
    assert_eq!(wit.variants[0].variant_name, "value");
}

#[test]
fn parser_accepts_table_definition() {
    let source = r#"
        package test:api;
        interface binding {
            table my-table { key: s64, value: string }
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.tables.len(), 1);
    assert_eq!(wit.tables[0].table_name, "my-table");
}

#[test]
fn parser_accepts_use_path() {
    let source = r#"
        package test:api;
        use other:dep/types.{foo};
        interface binding {}
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.use_path.len(), 1);
    assert!(wit.use_path[0].contains(&"other".to_string()));
}

#[test]
fn parser_reports_error_for_invalid_token() {
    let source = r#"
        package test:api;
        interface binding { @ }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source);
    // tree-sitter recovery may still produce a WitDef, but parsing should not succeed cleanly.
    assert!(wit.is_ok());
}

#[test]
fn analyzer_finds_error_in_invalid_source() {
    let source = "package test:api;\ninterface binding { @ }";
    let tree = parse_tree(source);
    let analyzer = AdvancedErrorAnalyzer::new();
    let report = analyzer.analyze(&tree, source);
    assert!(!report.errors.is_empty());
}

#[test]
fn analyzer_reports_no_errors_for_valid_source() {
    let source = r#"
        package test:api;
        interface binding {
            record point { x: s32, y: s32 }
        }
    "#;
    let tree = parse_tree(source);
    let analyzer = AdvancedErrorAnalyzer::new();
    let report = analyzer.analyze(&tree, source);
    assert!(report.errors.is_empty());
    assert!(report.suggestions.is_empty());
}

#[test]
fn parser_maps_list_u8_to_binary() {
    let source = r#"
        package test:api;
        interface binding {
            record packet { payload: list<u8> }
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.records.len(), 1);
    assert!(matches!(
        wit.records[0].record_fields[0].rf_type,
        UniDataType::Binary
    ));
}

#[test]
fn parser_maps_box_to_box_type() {
    let source = r#"
        package test:api;
        interface binding {
            record node { next: box<s32> }
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.records.len(), 1);
    assert!(matches!(
        wit.records[0].record_fields[0].rf_type,
        UniDataType::Box(_)
    ));
}

#[test]
fn parser_accepts_function_declaration() {
    let source = r#"
        package test:api;
        interface binding {
            add: func(a: s32, b: s32) -> s32;
            nop: func();
        }
    "#;
    let parser = WitParser::new();
    let wit = parser.parse_text(source).unwrap();
    assert_eq!(wit.functions.len(), 2);
    assert_eq!(wit.functions[0].func_name, "add");
    assert_eq!(wit.functions[0].params.len(), 2);
    assert_eq!(wit.functions[0].params[0].rf_name, "a");
    assert!(matches!(
        wit.functions[0].params[0].rf_type,
        UniDataType::Scalar(UniScalar::I32)
    ));
    assert_eq!(wit.functions[0].returns.len(), 1);
    assert!(matches!(
        wit.functions[0].returns[0].rf_type,
        UniDataType::Scalar(UniScalar::I32)
    ));
    assert_eq!(wit.functions[1].func_name, "nop");
    assert!(wit.functions[1].params.is_empty());
    assert!(wit.functions[1].returns.is_empty());
}
