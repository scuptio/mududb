//! WIT interface parser built on tree-sitter.

mod enum_variant;
mod func;
mod record;
mod table;
mod ty;

use crate::src_gen::wit_def::WitDef;
use crate::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_def::assign_field_numbers;
use std::collections::HashSet;
use tree_sitter::{Language, Node, Parser, Point, Tree};
use tree_sitter_wit;

/// Parser for WIT interface definitions.
pub struct WitParser {}

fn wit_language() -> Language {
    tree_sitter_wit::LANGUAGE.into()
}

/// Context holding the source text during parsing.
pub struct ParseContext {
    text: String,
}

impl ParseContext {
    fn text(&self) -> &str {
        &self.text
    }

    fn text_of_node(&self, node: &Node) -> RS<String> {
        Ok(node
            .utf8_text(self.text.as_bytes())
            .map_err(|e| mudu_error!(ErrorCode::Parse, "faile to convert to utf8", e))?
            .trim()
            .to_string())
    }
}

/// Analyzer that collects parse errors and suggests fixes.
pub struct AdvancedErrorAnalyzer {
    common_errors: HashSet<String>,
}

impl Default for AdvancedErrorAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl AdvancedErrorAnalyzer {
    /// Create a new analyzer.
    pub fn new() -> Self {
        let mut common_errors = HashSet::new();
        common_errors.insert("missing".to_string());
        common_errors.insert("ERROR".to_string());

        Self { common_errors }
    }

    /// Analyze a parse tree and produce a report.
    pub fn analyze(&self, tree: &Tree, source: &str) -> AnalysisReport {
        let root = tree.root_node();
        let errors = self.find_all_errors(root);

        let mut report = AnalysisReport::new();

        for error_node in errors {
            let error_info = self.analyze_error(&error_node, source);
            report.add_error(error_info);

            // try suggest
            if let Some(suggestion) = self.suggest_fix(&error_node, source) {
                report.add_suggestion(suggestion);
            }
        }

        report
    }

    fn find_all_errors<'a>(&self, node: Node<'a>) -> Vec<Node<'a>> {
        let mut errors = Vec::new();
        let mut stack = vec![node];

        while let Some(node) = stack.pop() {
            if self.is_error_node(&node) {
                errors.push(node);
            }

            let mut child = node.child(0);
            while let Some(c) = child {
                stack.push(c);
                child = c.next_sibling();
            }
        }

        errors
    }

    fn is_error_node(&self, node: &Node) -> bool {
        node.is_error() || node.is_missing() || self.common_errors.contains(node.kind())
    }

    fn analyze_error(&self, node: &Node, source: &str) -> ErrorInfo {
        let text = if node.start_byte() < source.len() && node.end_byte() <= source.len() {
            &source[node.start_byte()..node.end_byte()]
        } else {
            ""
        };

        // assign error type
        let error_type = self.determine_error_type(node);

        ErrorInfo {
            node_type: node.kind().to_string(),
            start: node.start_position(),
            end: node.end_position(),
            text: text.to_string(),
            error_type,
        }
    }

    fn determine_error_type(&self, node: &Node) -> ErrorType {
        if node.is_missing() {
            ErrorType::MissingToken
        } else if node.child_count() == 0 && node.kind() == "ERROR" {
            ErrorType::UnexpectedToken
        } else {
            ErrorType::SyntaxError
        }
    }

    fn suggest_fix(&self, node: &Node, _source: &str) -> Option<String> {
        if node.is_missing() {
            if let Some(parent) = node.parent() {
                let _ = parent.kind();
                None
            } else {
                None
            }
        } else {
            None
        }
    }
}

/// Classification of parse errors.
#[derive(Debug)]
pub enum ErrorType {
    /// A token was expected but missing.
    MissingToken,
    /// An unexpected token was encountered.
    UnexpectedToken,
    /// A generic syntax error.
    SyntaxError,
    /// Any other error.
    Other,
}

/// Description of a single parse error.
#[derive(Debug)]
pub struct ErrorInfo {
    /// Tree-sitter node kind.
    pub node_type: String,
    /// Start position in the source.
    pub start: Point,
    /// End position in the source.
    pub end: Point,
    /// Text of the offending node.
    pub text: String,
    /// Classified error type.
    pub error_type: ErrorType,
}

/// Report produced by [`AdvancedErrorAnalyzer`].
pub struct AnalysisReport {
    /// Errors found during analysis.
    pub errors: Vec<ErrorInfo>,
    /// Suggested fixes, if any.
    pub suggestions: Vec<String>,
}

impl Default for AnalysisReport {
    fn default() -> Self {
        Self::new()
    }
}

impl AnalysisReport {
    /// Create an empty report.
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    /// Add an error to the report.
    pub fn add_error(&mut self, error: ErrorInfo) {
        self.errors.push(error);
    }

    /// Add a suggestion to the report.
    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }

    /// Print the report to stdout.
    pub fn print(&self, _: &str) {
        if self.errors.is_empty() {
            println!("✅ parse success, no errors");
            return;
        }
        println!("{}", self.message());
    }

    /// Build a human-readable, single-string summary of the report.
    ///
    /// Each error is rendered as `<kind> at row <r>, column <c>: '<text>'`
    /// (row/column are 1-based), followed by any suggestions.
    pub fn message(&self) -> String {
        let mut out = format!("find {} parse error(s)", self.errors.len());
        for (i, error) in self.errors.iter().enumerate() {
            out.push_str(&format!(
                "\n{}. {} error at row {}, column {}",
                i + 1,
                match error.error_type {
                    ErrorType::MissingToken => "missing token",
                    ErrorType::UnexpectedToken => "unexpected token",
                    ErrorType::SyntaxError => "syntax error",
                    _ => "unknown error",
                },
                error.start.row + 1,
                error.start.column + 1,
            ));
            if !error.text.is_empty() {
                out.push_str(&format!(", content: '{}'", error.text));
            }
        }
        if !self.suggestions.is_empty() {
            out.push_str("\nsuggestion:");
            for suggestion in &self.suggestions {
                out.push_str(&format!("\n   - {}", suggestion));
            }
        }
        out
    }
}
impl Default for WitParser {
    fn default() -> Self {
        Self::new()
    }
}

impl WitParser {
    /// Create a new parser.
    pub fn new() -> WitParser {
        Self {}
    }

    /// Parse a WIT source string into a [`WitDef`].
    pub fn parse_text(&self, text: &str) -> RS<WitDef> {
        let mut parser = Parser::new();
        parser
            .set_language(&wit_language())
            .map_err(|e| mudu_error!(ErrorCode::Parse, "set parser language error", e))?;
        let tree = parser
            .parse(text, None)
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse wit text error"))?;
        let node = tree.root_node();
        let mut wit_dat = WitDef::default();
        if node.has_error() {
            let analyzer = AdvancedErrorAnalyzer::new();
            let report = analyzer.analyze(&tree, text);
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!("parse wit text error: {}", report.message())
            ));
        }
        let context = ParseContext {
            text: text.to_string(),
        };
        self.traverse_tree(&context, &node, &mut wit_dat)?;
        Self::assign_wit_field_numbers(&mut wit_dat);
        Ok(wit_dat)
    }

    /// Assign 1-based field numbers in declaration order to record fields,
    /// table key/value fields and function params/returns (protobuf-style
    /// field numbers, first field = 1).
    fn assign_wit_field_numbers(wit_dat: &mut WitDef) {
        for record in &mut wit_dat.records {
            assign_field_numbers(&mut record.record_fields);
        }
        for table in &mut wit_dat.tables {
            assign_field_numbers(&mut table.table_key);
            assign_field_numbers(&mut table.table_value);
        }
        for func in &mut wit_dat.functions {
            assign_field_numbers(&mut func.params);
            assign_field_numbers(&mut func.returns);
        }
    }

    fn traverse_tree(&self, context: &ParseContext, node: &Node, wit_dat: &mut WitDef) -> RS<()> {
        let kind = node.kind();
        match kind {
            ts_const::ts_kind_name::S_TOPLEVEL_USE_ITEM => {
                let use_path = self.visit_use_item(context, node)?;
                wit_dat.use_path.push(use_path);
            }
            ts_const::ts_kind_name::S_INTERFACE_ITEM => {
                let interface_name = self.visit_interface_item(context, node)?;
                wit_dat.interface.push(interface_name);
            }
            ts_const::ts_kind_name::S_TABLE_ITEM => {
                let table_def = self.visit_table_item(context, node)?;
                wit_dat.tables.push(table_def);
            }
            ts_const::ts_kind_name::S_RECORD_ITEM => {
                let record_def = self.visit_record_item(context, node)?;
                wit_dat.records.push(record_def);
            }
            ts_const::ts_kind_name::S_ENUM_ITEMS => {
                let enum_def = self.visit_enum_item(context, node)?;
                wit_dat.enums.push(enum_def);
            }
            ts_const::ts_kind_name::S_VARIANT_ITEMS => {
                let variant_def = self.visit_variant_item(context, node)?;
                wit_dat.variants.push(variant_def);
            }
            ts_const::ts_kind_name::S_FUNC_ITEM => {
                let func_def = self.visit_func_item(context, node)?;
                wit_dat.functions.push(func_def);
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_tree(context, &child, wit_dat)?;
        }
        Ok(())
    }

    fn visit_use_item(&self, context: &ParseContext, node: &Node) -> RS<Vec<String>> {
        let mut path = Vec::new();
        self.visit_use_item_inner(context, node, &mut path)?;
        Ok(path)
    }

    fn visit_interface_item(&self, context: &ParseContext, node: &Node) -> RS<String> {
        let name_node = expected_get_filed(node, ts_const::ts_field_name::NAME)?;
        let name = context.text_of_node(&name_node)?;
        Ok(name)
    }

    fn visit_use_item_inner(
        &self,
        context: &ParseContext,
        node: &Node,
        path: &mut Vec<String>,
    ) -> RS<()> {
        let mut cursor = node.walk();
        if node.kind() == ts_const::ts_kind_name::S_ID {
            let s = context.text_of_node(node)?;
            path.push(s);
        }
        for c in node.children(&mut cursor) {
            self.visit_use_item_inner(context, &c, path)?;
        }
        Ok(())
    }
}

fn expected_get_filed<'t>(node: &Node<'t>, name: &str) -> RS<Node<'t>> {
    let optional = node.child_by_field_name(name);
    optional.map_or_else(
        || {
            Err(mudu_error!(
                ErrorCode::Parse,
                format!("parse error, expected get field {}", name)
            ))
        },
        Ok,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod test {
    use crate::src_gen::wit_parser::WitParser;
    use mudu_utils::this_file;

    use std::path::PathBuf;

    // Miri cannot execute FFI calls into the tree-sitter C parser, so skip this
    // test under Miri. WIT parsing is still exercised by normal `cargo test`.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn test() {
        let path = PathBuf::from(this_file!());
        let path = path.parent().unwrap().to_path_buf().join("contract.wit");
        let str = mudu_sys::fs::sync::sync_read_to_string(path).unwrap();
        let parser = WitParser {};
        let wit_dat = parser.parse_text(&str).unwrap();
        println!("{:#?}", wit_dat);
    }
}
