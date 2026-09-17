//! Tree-sitter parser for the C# front-end.

use crate::common::ident::is_valid_identifier;
use crate::common::type_registry::TypeRegistry;
use crate::csharp::procedure::{CsParam, CsProcedure, CsValueType};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_snake_case;
use tree_sitter::{Node, Parser};

/// Discover all `// mudu-proc` marked procedures in `code`.
///
/// A procedure is a `static` method immediately preceded by a `// mudu-proc`
/// line comment (only whitespace between the comment and the method). The
/// first parameter must be the session OID (`MuduOid`); subsequent parameters
/// are scalar procedure arguments or `--type-wit` record/enum types
/// (`type_registry`). The return type may be any scalar except `MuduOid`,
/// or a `--type-wit` record/enum type.
pub fn discover_procedures(
    code: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<CsProcedure>> {
    let mut parser = Parser::new();
    let language = tree_sitter_c_sharp::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load C# grammar error", e))?;
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse C# source error"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(mudu_error!(ErrorCode::Parse, "C# source has syntax error"));
    }

    let mut comments = Vec::new();
    let mut methods = Vec::new();
    collect_comments_and_methods(root, &mut comments, &mut methods);

    let mut procedures = Vec::new();
    for method in methods {
        if !has_mudu_proc_label(code, method, &comments)? {
            continue;
        }
        let procedure = parse_method_declaration(code, method, type_registry)?;
        validate_procedure_signature(&procedure)?;
        if procedures
            .iter()
            .any(|p: &CsProcedure| p.name == procedure.name)
        {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "duplicate C# procedure",
                procedure.name
            ));
        }
        procedures.push(procedure);
    }
    Ok(procedures)
}

fn collect_comments_and_methods<'tree>(
    node: Node<'tree>,
    comments: &mut Vec<Node<'tree>>,
    methods: &mut Vec<Node<'tree>>,
) {
    match node.kind() {
        "comment" => comments.push(node),
        "method_declaration" => methods.push(node),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comments_and_methods(child, comments, methods);
    }
}

fn has_mudu_proc_label(code: &str, method: Node, comments: &[Node]) -> RS<bool> {
    let Some(comment) = comments
        .iter()
        .rev()
        .find(|comment| comment.end_byte() <= method.start_byte())
    else {
        return Ok(false);
    };
    if !node_text(code, *comment)?.contains("mudu-proc") {
        return Ok(false);
    }
    let between = &code[comment.end_byte()..method.start_byte()];
    Ok(between.trim().is_empty())
}

fn parse_method_declaration(
    code: &str,
    method: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<CsProcedure> {
    let name_node = method
        .child_by_field_name("name")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing C# method name"))?;
    let method_name = node_text(code, name_node)?;
    if !has_static_modifier(code, method)? {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C# procedure method must be static",
            method_name.clone()
        ));
    }

    let params_node = method.child_by_field_name("parameters").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing C# parameter list",
            method_name.clone()
        )
    })?;
    let params = parse_parameters(code, params_node, type_registry)?;
    if params.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C# procedure must have at least one MuduOid session parameter",
            method_name.clone()
        ));
    }
    let session_arg = params[0].name.clone();
    if !params[0].value_type.is_oid() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C# procedure first parameter must be MuduOid (the session)",
            method_name.clone()
        ));
    }
    if params.iter().skip(1).any(|param| param.value_type.is_oid()) {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C# procedure supports MuduOid only for the leading session parameter",
            method_name.clone()
        ));
    }

    let return_node = method.child_by_field_name("returns").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing C# return type",
            method_name.clone()
        )
    })?;
    let return_type = node_text(code, return_node)?;
    let return_value_type = parse_return_type(&return_type, type_registry)?;
    Ok(CsProcedure {
        name: to_snake_case(&method_name),
        method_name,
        params,
        return_type,
        return_value_type,
        session_arg,
    })
}

fn has_static_modifier(code: &str, method: Node) -> RS<bool> {
    let mut cursor = method.walk();
    for child in method.children(&mut cursor) {
        if child.kind() == "modifier" && node_text(code, child)? == "static" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_parameters(
    code: &str,
    params_node: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<CsParam>> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        if child.kind() != "parameter" {
            continue;
        }
        params.push(parse_parameter(code, child, type_registry)?);
    }
    Ok(params)
}

fn parse_parameter(
    code: &str,
    param_node: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<CsParam> {
    let name_node = param_node
        .child_by_field_name("name")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing C# parameter name"))?;
    let type_node = param_node
        .child_by_field_name("type")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing C# parameter type"))?;
    let name = node_text(code, name_node)?;
    let ty = node_text(code, type_node)?;
    let value_type = CsValueType::resolve(&ty, type_registry).map_err(|e| {
        mudu_error!(
            ErrorCode::Parse,
            format!("unsupported C# procedure parameter type '{ty}': {e}")
        )
    })?;
    Ok(CsParam {
        name,
        ty,
        value_type,
    })
}

/// The return type resolves like a parameter type: any scalar except
/// `MuduOid`, or a `--type-wit` record/enum type.
fn parse_return_type(return_type: &str, type_registry: Option<&TypeRegistry>) -> RS<CsValueType> {
    let value_type = CsValueType::resolve(return_type, type_registry).map_err(|e| {
        mudu_error!(
            ErrorCode::Parse,
            format!("unsupported C# procedure return type '{return_type}': {e}")
        )
    })?;
    if value_type.is_oid() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C# procedure result does not support MuduOid"
        ));
    }
    Ok(value_type)
}

fn node_text(code: &str, node: Node) -> RS<String> {
    node.utf8_text(code.as_bytes())
        .map(|text| text.to_string())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode C# source text error", e))
}

fn validate_procedure_signature(procedure: &CsProcedure) -> RS<()> {
    if !is_valid_identifier(&procedure.method_name)
        || procedure
            .params
            .iter()
            .any(|param| !is_valid_identifier(&param.name))
    {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid C# procedure identifier",
            procedure.method_name.clone()
        ));
    }
    Ok(())
}

#[cfg(all(test, not(miri)))]
#[path = "parser_test.rs"]
mod parser_test;
