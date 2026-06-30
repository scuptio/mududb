//! Tree-sitter parser for the AssemblyScript front-end.

use crate::assemblyscript::procedure::{AsParam, AsProcedure, AsValueType, normalize_type_name};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::{Node, Parser, TreeCursor};

/// Discover all `/**mudu-proc*/` annotated procedures in `code`.
pub fn discover_procedures(code: &str) -> RS<Vec<AsProcedure>> {
    let mut parser = Parser::new();
    let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT;
    parser
        .set_language(&language.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load TypeScript grammar error", e))?;
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse AssemblyScript source error"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "AssemblyScript source has syntax error"
        ));
    }

    let mut comments = Vec::new();
    let mut functions = Vec::new();
    collect_comments_and_functions(root, &mut comments, &mut functions);

    let mut procedures = Vec::new();
    for function in functions {
        if !has_mudu_proc_label(code, function, &comments)? {
            continue;
        }
        let procedure = parse_function_declaration(code, function)?;
        validate_procedure_signature(&procedure)?;
        if procedures
            .iter()
            .any(|p: &AsProcedure| p.name == procedure.name)
        {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "duplicate AssemblyScript procedure",
                procedure.name
            ));
        }
        procedures.push(procedure);
    }
    Ok(procedures)
}

fn collect_comments_and_functions<'tree>(
    node: Node<'tree>,
    comments: &mut Vec<Node<'tree>>,
    functions: &mut Vec<Node<'tree>>,
) {
    match node.kind() {
        "comment" => comments.push(node),
        "function_declaration" => functions.push(node),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comments_and_functions(child, comments, functions);
    }
}

fn has_mudu_proc_label(code: &str, function: Node, comments: &[Node]) -> RS<bool> {
    let Some(comment) = comments
        .iter()
        .rev()
        .find(|comment| comment.end_byte() <= function.start_byte())
    else {
        return Ok(false);
    };
    if !node_text(code, *comment)?.contains("mudu-proc") {
        return Ok(false);
    }
    let between = &code[comment.end_byte()..function.start_byte()];
    Ok(is_allowed_between_label_and_function(between))
}

fn is_allowed_between_label_and_function(input: &str) -> bool {
    input
        .split_whitespace()
        .all(|token| matches!(token, "export" | "default"))
}

fn parse_function_declaration(code: &str, function: Node) -> RS<AsProcedure> {
    let name_node = function
        .child_by_field_name("name")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing AssemblyScript function name"))?;
    let name = node_text(code, name_node)?;

    let params_node = function.child_by_field_name("parameters").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing AssemblyScript parameter list",
            name.clone()
        )
    })?;
    let params = parse_parameters(code, params_node)?;
    if params.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "AssemblyScript procedure must have at least one Oid parameter"
        ));
    }
    let id_arg = params[0].name.clone();
    if !params[0].value_type.is_oid() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "AssemblyScript procedure first parameter must be Oid"
        ));
    }

    let return_node = function.child_by_field_name("return_type").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing AssemblyScript return type",
            name.clone()
        )
    })?;
    let return_type = normalize_type_annotation(&node_text(code, return_node)?);
    let (return_value_type, returns_result) = parse_return_type(&return_type)?;
    Ok(AsProcedure {
        name,
        params,
        return_type,
        return_value_type,
        returns_result,
        id_arg,
    })
}

fn parse_parameters(code: &str, params_node: Node) -> RS<Vec<AsParam>> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        if !matches!(
            child.kind(),
            "required_parameter" | "optional_parameter" | "rest_pattern"
        ) {
            continue;
        }
        params.push(parse_parameter(code, child)?);
    }
    Ok(params)
}

fn parse_parameter(code: &str, param_node: Node) -> RS<AsParam> {
    let name_node = param_node
        .child_by_field_name("pattern")
        .or_else(|| param_node.child_by_field_name("name"))
        .or_else(|| first_named_child_of_kind(param_node, "identifier"))
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing AssemblyScript parameter name"))?;
    let type_node = param_node
        .child_by_field_name("type")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing AssemblyScript parameter type"))?;
    let name = node_text(code, name_node)?;
    let ty = normalize_type_annotation(&node_text(code, type_node)?);
    let value_type = AsValueType::parse(&ty).ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "unsupported AssemblyScript procedure parameter type",
            ty.clone()
        )
    })?;
    Ok(AsParam {
        name,
        ty,
        value_type,
    })
}

fn parse_return_type(return_type: &str) -> RS<(AsValueType, bool)> {
    if let Some(inner) = strip_result_type(return_type) {
        let value_type = AsValueType::parse(&inner).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                "unsupported AssemblyScript procedure result type",
                return_type.to_string()
            )
        })?;
        return Ok((value_type, true));
    }

    let value_type = AsValueType::parse(return_type).ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "unsupported AssemblyScript procedure return type",
            return_type.to_string()
        )
    })?;
    Ok((value_type, false))
}

fn strip_result_type(input: &str) -> Option<String> {
    let normalized = normalize_type_name(input);
    for prefix in ["result<"] {
        if normalized.starts_with(prefix) && normalized.ends_with('>') {
            return Some(normalized[prefix.len()..normalized.len() - 1].to_string());
        }
    }
    None
}

fn first_named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor: TreeCursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn normalize_type_annotation(input: &str) -> String {
    input.trim().trim_start_matches(':').trim().to_string()
}

fn node_text(code: &str, node: Node) -> RS<String> {
    node.utf8_text(code.as_bytes())
        .map(|text| text.to_string())
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "decode AssemblyScript source text error",
                e
            )
        })
}

fn validate_procedure_signature(procedure: &AsProcedure) -> RS<()> {
    if !is_valid_identifier(&procedure.name)
        || !is_valid_identifier(&procedure.id_arg)
        || procedure
            .params
            .iter()
            .any(|param| !is_valid_identifier(&param.name))
    {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid AssemblyScript procedure identifier",
            procedure.name.clone()
        ));
    }
    Ok(())
}

fn is_valid_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[cfg(all(test, not(miri)))]
#[path = "parser_test.rs"]
mod parser_test;
