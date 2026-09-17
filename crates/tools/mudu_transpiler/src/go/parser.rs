//! Tree-sitter parser for the Go front-end.

use crate::common::ident::is_valid_identifier;
use crate::common::type_registry::TypeRegistry;
use crate::go::procedure::{GoParam, GoProcedure, GoValueType};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_snake_case;
use tree_sitter::{Node, Parser, TreeCursor};

/// Discover all `// mudu-proc` marked procedures in `code`.
///
/// A procedure is a top-level `func` immediately preceded by a `// mudu-proc`
/// line comment (only whitespace between the comment and the function). The
/// first parameter must be the session OID (`muduOid`); subsequent parameters
/// are scalar procedure arguments, user-defined types from the `--type-wit`
/// registry, or `*T` optional forms of those. The result must be `(T, error)`
/// with `T` a supported scalar or record type.
pub fn discover_procedures(
    code: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<GoProcedure>> {
    let mut parser = Parser::new();
    let language = tree_sitter_go::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load Go grammar error", e))?;
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse Go source error"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(mudu_error!(ErrorCode::Parse, "Go source has syntax error"));
    }

    let mut comments = Vec::new();
    collect_comments(root, &mut comments);

    let mut procedures = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        if child.kind() != "function_declaration" {
            continue;
        }
        if !has_mudu_proc_label(code, child, &comments)? {
            continue;
        }
        let procedure = parse_function_declaration(code, child, type_registry)?;
        validate_procedure_signature(&procedure)?;
        if procedures
            .iter()
            .any(|p: &GoProcedure| p.name == procedure.name)
        {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "duplicate Go procedure",
                procedure.name
            ));
        }
        procedures.push(procedure);
    }
    Ok(procedures)
}

fn collect_comments<'tree>(node: Node<'tree>, comments: &mut Vec<Node<'tree>>) {
    if node.kind() == "comment" {
        comments.push(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comments(child, comments);
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
    Ok(between.trim().is_empty())
}

fn parse_function_declaration(
    code: &str,
    function: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<GoProcedure> {
    let name_node = function
        .child_by_field_name("name")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing Go function name"))?;
    let func_name = node_text(code, name_node)?;

    let params_node = function.child_by_field_name("parameters").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing Go parameter list",
            func_name.clone()
        )
    })?;
    let params = parse_parameters(code, params_node, type_registry)?;
    if params.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure must have at least one muduOid session parameter",
            func_name.clone()
        ));
    }
    let session_arg = params[0].name.clone();
    if !params[0].value_type.is_oid() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure first parameter must be muduOid (the session)",
            func_name.clone()
        ));
    }
    if params.iter().skip(1).any(|param| param.value_type.is_oid()) {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure supports muduOid only for the leading session parameter",
            func_name.clone()
        ));
    }

    let (return_type, return_value_type) = parse_result(code, function, &func_name, type_registry)?;
    Ok(GoProcedure {
        name: to_snake_case(&func_name),
        func_name,
        params,
        return_type,
        return_value_type,
        session_arg,
    })
}

fn parse_parameters(
    code: &str,
    params_node: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<GoParam>> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        match child.kind() {
            "parameter_declaration" => {
                parse_parameter_declaration(code, child, type_registry, &mut params)?
            }
            // `...T` splats cannot be mapped to the positional `param_list`
            // wire shape.
            "variadic_parameter_declaration" => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "unsupported Go procedure parameter kind (variadic)",
                    node_text(code, child)?
                ));
            }
            _ => {}
        }
    }
    Ok(params)
}

/// One `parameter_declaration` is `name..., type`; the name list may declare
/// several parameters of the same type (`a, b int64`).
fn parse_parameter_declaration(
    code: &str,
    decl: Node,
    type_registry: Option<&TypeRegistry>,
    params: &mut Vec<GoParam>,
) -> RS<()> {
    let type_node = decl
        .child_by_field_name("type")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing Go parameter type"))?;
    let ty = node_text(code, type_node)?;
    let value_type = GoValueType::resolve(&ty, type_registry).map_err(|e| {
        mudu_error!(
            ErrorCode::Parse,
            format!("unsupported Go procedure parameter type '{ty}': {e}")
        )
    })?;

    let mut cursor: TreeCursor = decl.walk();
    let mut found = false;
    for name_node in decl.children_by_field_name("name", &mut cursor) {
        found = true;
        params.push(GoParam {
            name: node_text(code, name_node)?,
            ty: ty.clone(),
            value_type: value_type.clone(),
        });
    }
    if !found {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "missing Go parameter name",
            ty
        ));
    }
    Ok(())
}

/// The result convention is `(T, error)`: tree-sitter exposes the parenthesized
/// form as a `parameter_list`; a bare single-type result is rejected because
/// the generated adapter needs the error arm. `T` may be a scalar or a
/// `--type-wit` record type; enum and option returns are rejected (v1: the
/// wire result carries no ordinal/nullable metadata for them).
fn parse_result(
    code: &str,
    function: Node,
    func_name: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<(String, GoValueType)> {
    let Some(result_node) = function.child_by_field_name("result") else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure result must be (T, error)",
            func_name.to_string()
        ));
    };
    if result_node.kind() != "parameter_list" {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure result must be (T, error)",
            node_text(code, result_node)?
        ));
    }
    let mut types = Vec::new();
    let mut cursor = result_node.walk();
    for child in result_node.named_children(&mut cursor) {
        if child.kind() != "parameter_declaration" {
            continue;
        }
        let type_node = child
            .child_by_field_name("type")
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing Go result type"))?;
        types.push(node_text(code, type_node)?);
    }
    if types.len() != 2 || types[1].trim() != "error" {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Go procedure result must be (T, error)",
            func_name.to_string()
        ));
    }
    let return_type = types[0].clone();
    let return_value_type = GoValueType::resolve(&return_type, type_registry).map_err(|e| {
        mudu_error!(
            ErrorCode::Parse,
            format!("unsupported Go procedure result type '{return_type}': {e}")
        )
    })?;
    match &return_value_type {
        GoValueType::ObjectId => {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "Go procedure result does not support muduOid",
                func_name.to_string()
            ));
        }
        GoValueType::Enum(custom) => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "Go procedure result does not support the enum type '{}' (v1: enum parameters only)",
                    custom.name
                )
            ));
        }
        GoValueType::Option(_) => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "Go procedure result does not support the option type '{return_type}' (v1: option parameters only)"
                )
            ));
        }
        _ => {}
    }
    Ok((return_type, return_value_type))
}

fn node_text(code: &str, node: Node) -> RS<String> {
    node.utf8_text(code.as_bytes())
        .map(|text| text.to_string())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode Go source text error", e))
}

fn validate_procedure_signature(procedure: &GoProcedure) -> RS<()> {
    if !is_valid_identifier(&procedure.func_name)
        || procedure
            .params
            .iter()
            .any(|param| !is_valid_identifier(&param.name))
    {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid Go procedure identifier",
            procedure.func_name.clone()
        ));
    }
    Ok(())
}

#[cfg(all(test, not(miri)))]
#[path = "parser_test.rs"]
mod parser_test;
