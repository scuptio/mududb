use std::collections::HashSet;
use crate::ts_const;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_primitive::UniPrimitive;
use mudu_binding::universal::uni_def::{EnumCase, UniEnumDef, UniRecordDef, RecordField, VariantCase, UniVariantDef};
use tree_sitter::{Language, Node, Parser, Point, Tree};
use mudu_binding::universal::uni_result_type::UniResultType;
use tree_sitter_wit;
use crate::src_gen::wit_def::WitDef;

pub struct WitParser {

}

fn wit_language() -> Language {
    tree_sitter_wit::LANGUAGE.into()
}

pub struct ParseContext {
    text:String,
}

impl ParseContext {
    fn text(&self) -> &str   {
        &self.text
    }

    fn text_of_node(&self, node: &Node) -> String {
        node.utf8_text(self.text.as_bytes())
            .unwrap()
            .trim()
            .to_string()
    }
}


pub struct AdvancedErrorAnalyzer {
    common_errors: HashSet<String>,
}

impl AdvancedErrorAnalyzer {
    pub fn new() -> Self {
        let mut common_errors = HashSet::new();
        common_errors.insert("missing".to_string());
        common_errors.insert("ERROR".to_string());

        Self { common_errors }
    }

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
                match parent.kind() {
                    _ => None,
                }
            } else {
                None
            }
        } else {
            None
        }
    }
}

#[derive(Debug)]
pub enum ErrorType {
    MissingToken,
    UnexpectedToken,
    SyntaxError,
    Other,
}

#[derive(Debug)]
pub struct ErrorInfo {
    pub node_type: String,
    pub start: Point,
    pub end: Point,
    pub text: String,
    pub error_type: ErrorType,
}

pub struct AnalysisReport {
    pub errors: Vec<ErrorInfo>,
    pub suggestions: Vec<String>,
}

impl AnalysisReport {
    pub fn new() -> Self {
        Self {
            errors: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: ErrorInfo) {
        self.errors.push(error);
    }

    pub fn add_suggestion(&mut self, suggestion: String) {
        self.suggestions.push(suggestion);
    }

    pub fn print(&self, _: &str) {
        if self.errors.is_empty() {
            println!("✅ parse success, no errors");
            return;
        }

        println!("❌ find {} error(s)", self.errors.len());

        for (i, error) in self.errors.iter().enumerate() {
            println!("\n{}. {} error", i + 1, match error.error_type {
                ErrorType::MissingToken => "missing token",
                ErrorType::UnexpectedToken => "unexpected token",
                ErrorType::SyntaxError => "syntax error",
                _ => "unknown error",
            });

            println!("   location: {} row, {} column",
                     error.start.row + 1,
                     error.start.column + 1
            );

            if !error.text.is_empty() {
                println!("   content: '{}'", error.text);
            }
        }

        if !self.suggestions.is_empty() {
            println!("\n💡 suggestion:");
            for suggestion in &self.suggestions {
                println!("   - {}", suggestion);
            }
        }
    }
}
impl WitParser {

    pub fn new() -> WitParser {
        Self {}
    }
    pub fn parse_text(&self, text: &str) -> RS<WitDef> {
        let mut parser = Parser::new();
        parser.set_language(&wit_language()).unwrap();
        let tree = parser.parse(text, None).unwrap();
        let node = tree.root_node();
        let mut wit_dat = WitDef::default();
        if node.has_error() {
            let analyzer = AdvancedErrorAnalyzer::new();
            let report = analyzer.analyze(&tree, text);
            report.print(&text);
        }
        let context = ParseContext { text: text.to_string() };
        self.traverse_tree(&context, &node, &mut wit_dat)?;
        Ok(wit_dat)
    }

    fn traverse_tree(&self, context:&ParseContext, node:&Node, wit_dat:&mut WitDef) -> RS<()> {
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
            ts_const::ts_kind_name::S_RECORD_ITEM => {
                let record_def = self.visit_record_item(context, &node)?;
                wit_dat.records.push(record_def);
            }
            ts_const::ts_kind_name::S_ENUM_ITEMS => {
                let enum_def = self.visit_enum_item(context, &node)?;
                wit_dat.enums.push(enum_def);
            }
            ts_const::ts_kind_name::S_VARIANT_ITEMS => {
                let variant_def = self.visit_variant_item(context, &node)?;
                wit_dat.variants.push(variant_def);
            }
            _ => {}
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse_tree(context, &child, wit_dat)?;
        }
        Ok(())
    }

    fn visit_use_item(&self, context: &ParseContext, node:&Node) -> RS<Vec<String>> {
        let mut path = Vec::new();
        self.visit_use_item_inner(context, node, &mut path)?;
        Ok(path)
    }

    fn visit_interface_item(&self, context: &ParseContext, node:&Node) -> RS<String> {
        let name_node = expected_get_filed(node, ts_const::ts_field_name::NAME)?;
        let name = context.text_of_node(&name_node);
        Ok(name)
    }
    fn visit_use_item_inner(&self, context: &ParseContext, node:&Node, path:&mut Vec<String>) -> RS<()> {
        let mut cursor = node.walk();
        if node.kind() == ts_const::ts_kind_name::S_ID {
            let s = context.text_of_node(node);
            path.push(s);
        }
        for c in node.children(&mut cursor) {
            self.visit_use_item_inner(context, &c, path)?;
        }
        Ok(())
    }
    fn visit_record_item(&self, context:&ParseContext, node:&Node) -> RS<UniRecordDef> {
        let mut record = UniRecordDef {
            record_comments: String::new(),
            record_name: String::new(),
            record_fields: Vec::new(),
        };
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            record.record_comments = comment.utf8_text(context.text().as_bytes())
                .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
                .to_string();
        }

        // record name
        let name_node = expected_get_filed(node, ts_const::ts_field_name::RECORD_NAME)?;
        record.record_name = name_node.utf8_text(context.text().as_bytes())
            .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
            .to_string();


        // fields
        let mut field_cursor = node.walk();
        for field_node in node.children_by_field_name(ts_const::ts_field_name::RECORD_FIELD, &mut field_cursor) {
            let field = self.visit_record_field(context, &field_node)?;
            record.record_fields.push(field);
        }
        Ok(record)
    }

    fn visit_record_field(&self, context: &ParseContext, node: &Node) -> RS<RecordField> {
        let mut field_comments = String::new();
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            field_comments = comment.utf8_text(context.text().as_bytes())
                .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
                .to_string();
        }
        let n_name = node.child_by_field_name(ts_const::ts_field_name::RECORD_FIELD_NAME)
            .map_or_else(||{ Err(m_error!(EC::ParseErr, "parse error, expected record field name")) }, |child| { Ok(child)})?;
        let field_name = context.text_of_node(&n_name);
        let n_type = node.child_by_field_name(ts_const::ts_field_name::RECORD_FIELD_TYPE)
            .map_or_else(||{ Err(m_error!(EC::ParseErr, "parse error, expected record field type")) }, |child| { Ok(child)})?;
        let field_type = self.visit_type(context, &n_type)?;
        Ok(RecordField {
            rf_comments: field_comments,
            rf_name: field_name,
            rf_type: field_type
        })
    }


    fn visit_enum_item(&self, context:&ParseContext, node:&Node) -> RS<UniEnumDef>{
        let mut enum_comments = String::new();
        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            enum_comments = comment.utf8_text(context.text().as_bytes())
                .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
                .to_string();
        }
        let mut enum_def = UniEnumDef {
            enum_comments,
            enum_name: String::new(),
            enum_cases: Vec::new(),
        };

        // get enum name
        if let Some(name_node) = node.child_by_field_name(ts_const::ts_field_name::ENUM_NAME) {
            enum_def.enum_name = context.text_of_node(&name_node);
        }

        let mut case_cursor = node.walk();
        for (i, case_node) in node.children_by_field_name(ts_const::ts_field_name::ENUM_CASE, &mut case_cursor).enumerate() {
            let mut comments = String::new();
            if let Some(comment) = case_node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
                comments = comment.utf8_text(context.text().as_bytes())
                    .map_err(|e|m_error!(EC::ParseErr, "parse error to retrieve comments", e))?
                    .to_string();
            }

            let node_case_name = expected_get_filed(&case_node, ts_const::ts_field_name::ENUM_CASE_NAME)?;
            let case_name = context.text_of_node(&node_case_name);
            let enum_case = EnumCase {
                ec_name: case_name,
                ec_comments: comments,
                ec_number: i as _,
            };
            enum_def.enum_cases.push(enum_case);
        }
        if enum_def.enum_cases.is_empty() {
            return Err(m_error!(EC::ParseErr, "enum_cases is empty"));
        }
        Ok(enum_def)
    }

    fn visit_variant_item(&self, context: &ParseContext, node: &Node) -> RS<UniVariantDef> {
        let mut variant_def = UniVariantDef {
            variant_comments: String::new(),
            variant_name: String::new(),
            variant_cases: Vec::new(),
        };

        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            let comments = comment.utf8_text(context.text().as_bytes())
                .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
                .to_string();
            variant_def.variant_comments = comments;
        }
        let name_node = expected_get_filed(node, ts_const::ts_field_name::VARIANT_NAME)?;
        variant_def.variant_name = context.text_of_node(&name_node);


        let mut case_cursor = node.walk();
        for case_node in node.children_by_field_name(ts_const::ts_field_name::VARIANT_CASE, &mut case_cursor) {
            let case = self.visit_variant_case(context, &case_node)?;
            variant_def.variant_cases.push(case);
        }
        Ok(variant_def)
    }

    fn visit_variant_case(&self, context: &ParseContext, node: &Node) -> RS<VariantCase> {
        let mut variant_case_def = VariantCase {
            vc_comments: String::new(),
            vc_case_name: String::new(),
            vc_case_type: None,
        };

        if let Some(comment) = node.child_by_field_name(ts_const::ts_field_name::COMMENT) {
            let comments = comment.utf8_text(context.text().as_bytes())
                .map_err(|e|m_error!(EC::ParseErr, "parse error", e))?
                .to_string();
            variant_case_def.vc_comments = comments;
        }

        let name_node = expected_get_filed(node, ts_const::ts_field_name::VARIANT_CASE_NAME)?;
        variant_case_def.vc_case_name = context.text_of_node(&name_node);

        if let Some(case_type) = node.child_by_field_name(ts_const::ts_field_name::VARIANT_CASE_TYPE) {
            let inner_type = self.visit_type(&context, &case_type)?;
            variant_case_def.vc_case_type = Some(inner_type);
        }
        Ok(variant_case_def)
    }

    fn visit_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let child =  node.child(0)
            .map_or_else(||{
                Err(m_error!(EC::ParseErr, "parse error, expected has a child of type node"))
            }, |n|{
                Ok(n)
            })?;
        let ty = self.parse_type_node(context, &child)?;
        Ok(ty)
    }

    fn parse_tuple_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let child = expected_get_filed(node, ts_const::ts_field_name::TUPLE_LIST)?;
        let mut cursor = child.walk();
        let mut vec = Vec::new();
        for n in child.children(&mut cursor) {
            if n.kind() == ts_const::ts_kind_name::S_TY {
                let wit_ty = self.visit_type(&context, &n)?;
                vec.push(wit_ty);
            }
        }
        Ok(UniDatType::Tuple(vec))
    }

    fn parse_list_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::LIST_INNER_TYPE)?;
        let ty_inner = self.visit_type(&context, &ty_node)?;
        Ok(UniDatType::Array(Box::new(ty_inner)))
    }

    fn parse_option_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::OPTION_INNER_TYPE)?;
        let ty_inner = self.visit_type(&context, &ty_node)?;
        Ok(UniDatType::Option(Box::new(ty_inner)))
    }

    fn parse_result_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let ty_err = node.child_by_field_name(ts_const::ts_field_name::RESULT_ERR_TYPE);
        let opt_err = if let Some(n) = ty_err {
            let ty_err_inner = self.visit_type(&context, &n)?;
            Some(Box::new(ty_err_inner))
        } else {
            None
        };

        let ty_ok = node.child_by_field_name(ts_const::ts_field_name::RESULT_OK_TYPE);
        let opt_ok = if let Some(n) = ty_ok {
            let ty_ok_inner = self.visit_type(&context, &n)?;
            Some(Box::new(ty_ok_inner))
        } else {
            None
        };

        Ok(UniDatType::Result(UniResultType { ok: opt_ok, err: opt_err }))
    }

    fn parse_custom_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let type_name = context.text_of_node(&node);
        Ok(UniDatType::Identifier(type_name))
    }
    fn parse_box_type(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let ty_node = expected_get_filed(node, ts_const::ts_field_name::BOX_INNER)?;
        let ty_inner = self.visit_type(&context, &ty_node)?;
        Ok(UniDatType::Box(Box::new(ty_inner)))
    }

    fn parse_type_node(&self, context: &ParseContext, node: &Node) -> RS<UniDatType> {
        let ty = match node.kind() {
            ts_const::ts_kind_name::S_BOOL => UniDatType::Primitive(UniPrimitive::Bool),
            ts_const::ts_kind_name::S_U8 => UniDatType::Primitive(UniPrimitive::U8),
            ts_const::ts_kind_name::S_U16 => UniDatType::Primitive(UniPrimitive::U16),
            ts_const::ts_kind_name::S_U32 => UniDatType::Primitive(UniPrimitive::U32),
            ts_const::ts_kind_name::S_U64 => UniDatType::Primitive(UniPrimitive::U64),
            ts_const::ts_kind_name::S_S8 => UniDatType::Primitive(UniPrimitive::U8),
            ts_const::ts_kind_name::S_S16 => UniDatType::Primitive(UniPrimitive::I16),
            ts_const::ts_kind_name::S_S32 => UniDatType::Primitive(UniPrimitive::I32),
            ts_const::ts_kind_name::S_S64 => UniDatType::Primitive(UniPrimitive::I64),
            ts_const::ts_kind_name::S_F32 => UniDatType::Primitive(UniPrimitive::F32),
            ts_const::ts_kind_name::S_F64 => UniDatType::Primitive(UniPrimitive::F64),
            ts_const::ts_kind_name::S_CHAR => UniDatType::Primitive(UniPrimitive::Char),
            ts_const::ts_kind_name::S_STRING => UniDatType::Primitive(UniPrimitive::String),
            ts_const::ts_kind_name::S_TUPLE => self.parse_tuple_type(context, node)?,
            ts_const::ts_kind_name::S_LIST => self.parse_list_type(context, node)?,
            ts_const::ts_kind_name::S_OPTION => self.parse_option_type(context, node)?,
            ts_const::ts_kind_name::S_RESULT => self.parse_result_type(context, node)?,
            ts_const::ts_kind_name::S_ID => self.parse_custom_type(context, node)?,
            ts_const::ts_kind_name::S_BOX => self.parse_box_type(context, node)?,
            _ => {
                println!("kind {}, text {}", node.kind(), context.text_of_node(&node));
                return Err(m_error!(EC::NotImplemented, "do not support type"))
            }
        };
        Ok(ty)
    }
}


fn expected_get_filed<'t>(node: &Node<'t>, name: &str) -> RS<Node<'t>> {
    let optional = node.child_by_field_name(name);
    optional
        .map_or_else(
            || {
                Err(m_error!(EC::NoneErr, format!("parse error, expected get field {}", name)))
            },
            |t|
                {
                    Ok(t)
                },
        )
}

#[cfg(test)]
mod test {
    use crate::src_gen::wit_parser::WitParser;
    use mudu::this_file;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn test() {
        let path = PathBuf::from(this_file!());
        let path = path.parent().unwrap().to_path_buf()
            .join("contract.wit");
        let str = fs::read_to_string(path).unwrap();
        let parser = WitParser{};
        let wit_dat = parser.parse_text(&str).unwrap();
        println!("{:#?}", wit_dat);
    }
}