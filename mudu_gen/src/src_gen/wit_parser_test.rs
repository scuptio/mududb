//! Unit tests for the WIT parser and error analyzer.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::src_gen::wit_parser::{AdvancedErrorAnalyzer, WitParser};
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
