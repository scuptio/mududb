//! Tree-sitter parser for the C front-end.

use crate::c::procedure::{CParam, CProcedure, CValueType};
use crate::common::ident::is_valid_identifier;
use crate::common::type_registry::TypeRegistry;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::{Node, Parser};

/// Discover all `// mudu-proc` marked procedures in `code`.
///
/// A procedure is a non-`static` function definition immediately preceded by
/// a `// mudu-proc (name: type, ...) -> type` line comment (only whitespace
/// between the comment and the function). The C ABI of a procedure
/// implementation is fixed (`int fn(const mudu_proc_param *, mudu_datum *,
/// mudu_error *)`), so the wire signature comes from the annotation; the
/// session OID is implicit in `mudu_proc_param.session`.
///
/// Annotation types resolve in two stages: the scalar table (`i64`/`f64`/
/// `string`), then the `--type-wit` registry (record/enum type names, plus
/// an `option<T>` wrapper for nullable parameters).
pub fn discover_procedures(
    code: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<CProcedure>> {
    let mut parser = Parser::new();
    let language = tree_sitter_c::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load C grammar error", e))?;
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse C source error"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(mudu_error!(ErrorCode::Parse, "C source has syntax error"));
    }

    let mut comments = Vec::new();
    let mut functions = Vec::new();
    collect_comments_and_functions(root, &mut comments, &mut functions);

    let mut procedures = Vec::new();
    for function in functions {
        let Some(comment) = nearest_preceding_comment(code, function, &comments)? else {
            continue;
        };
        let (params, return_type, return_value_type) = parse_annotation(&comment, type_registry)?;
        let procedure =
            parse_function_definition(code, function, params, return_type, return_value_type)?;
        validate_procedure_signature(&procedure)?;
        if procedures
            .iter()
            .any(|p: &CProcedure| p.name == procedure.name)
        {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "duplicate C procedure",
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
        "function_definition" => functions.push(node),
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_comments_and_functions(child, comments, functions);
    }
}

/// The marker comment of `function`: the nearest preceding comment, which
/// must carry a `mudu-proc` annotation with only whitespace between the
/// comment and the function.
fn nearest_preceding_comment(code: &str, function: Node, comments: &[Node]) -> RS<Option<String>> {
    let Some(comment) = comments
        .iter()
        .rev()
        .find(|comment| comment.end_byte() <= function.start_byte())
    else {
        return Ok(None);
    };
    let text = node_text(code, *comment)?;
    if !text.contains("mudu-proc") {
        return Ok(None);
    }
    let between = &code[comment.end_byte()..function.start_byte()];
    if !between.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(text))
}

/// Parse the `mudu-proc (name: type, ...) -> type` annotation of the marker
/// comment. The mini grammar is hand-parsed (types are plain tokens plus the
/// `option<T>` wrapper, so no nesting occurs); tree-sitter only locates the
/// function the annotation describes.
fn parse_annotation(
    comment: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<(Vec<CParam>, String, CValueType)> {
    let Some(marker) = comment.find("mudu-proc") else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "missing mudu-proc marker in annotation comment"
        ));
    };
    let rest = comment[marker + "mudu-proc".len()..].trim_start();
    let Some(after_open) = rest.strip_prefix('(') else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "malformed mudu-proc annotation (expected `(name: type, ...) -> type`)",
            comment.to_string()
        ));
    };
    let Some(close) = after_open.find(')') else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "malformed mudu-proc annotation (unclosed parameter list)",
            comment.to_string()
        ));
    };
    let inner = &after_open[..close];
    let after_close = after_open[close + 1..].trim_start();
    let Some(return_rest) = after_close.strip_prefix("->") else {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "malformed mudu-proc annotation (missing `->` return type)",
            comment.to_string()
        ));
    };
    // The return type runs to the end of the comment text; for a `//` line
    // comment that is the end of the line.
    let return_type = first_line(return_rest).trim().to_string();
    if return_type.split_whitespace().count() != 1 {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "malformed mudu-proc annotation return type",
            return_type
        ));
    }
    let return_value_type = CValueType::resolve(&return_type, type_registry).map_err(|e| {
        mudu_error!(
            ErrorCode::Parse,
            format!("unsupported mudu-proc annotation return type '{return_type}': {e}")
        )
    })?;
    match &return_value_type {
        CValueType::Enum(custom) => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "C procedure result does not support the enum type '{}' (v1: enum parameters only)",
                    custom.name
                )
            ));
        }
        CValueType::Option(_) => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "C procedure result does not support the option type '{return_type}' (v1: option parameters only)"
                )
            ));
        }
        _ => {}
    }

    let params = parse_annotation_params(inner, type_registry)?;
    Ok((params, return_type, return_value_type))
}

fn first_line(input: &str) -> &str {
    match input.find('\n') {
        Some(index) => &input[..index],
        None => input,
    }
}

fn parse_annotation_params(inner: &str, type_registry: Option<&TypeRegistry>) -> RS<Vec<CParam>> {
    let trimmed = inner.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    let mut params = Vec::new();
    for piece in trimmed.split(',') {
        let Some((name, ty)) = piece.split_once(':') else {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "malformed mudu-proc annotation parameter (expected `name: type`)",
                piece.trim().to_string()
            ));
        };
        let name = name.trim().to_string();
        let ty = ty.trim().to_string();
        if name.is_empty() || ty.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "malformed mudu-proc annotation parameter (expected `name: type`)",
                piece.trim().to_string()
            ));
        }
        let value_type = CValueType::resolve(&ty, type_registry).map_err(|e| {
            mudu_error!(
                ErrorCode::Parse,
                format!("unsupported mudu-proc annotation parameter type '{ty}': {e}")
            )
        })?;
        params.push(CParam {
            name,
            ty,
            value_type,
        });
    }
    Ok(params)
}

fn parse_function_definition(
    code: &str,
    function: Node,
    params: Vec<CParam>,
    return_type: String,
    return_value_type: CValueType,
) -> RS<CProcedure> {
    let declarator = function
        .child_by_field_name("declarator")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing C function declarator"))?;
    if declarator.kind() != "function_declarator" {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "unsupported C procedure declarator (must be a plain function)",
            node_text(code, declarator)?
        ));
    }
    let name_node = declarator
        .child_by_field_name("declarator")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing C function name"))?;
    if name_node.kind() != "identifier" {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "unsupported C procedure declarator (must be a plain function name)",
            node_text(code, name_node)?
        ));
    }
    let name = node_text(code, name_node)?;

    if is_static_function(code, function)? {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C procedure must not be static (the generated adapter calls it)",
            name.clone()
        ));
    }
    let c_return_type = function
        .child_by_field_name("type")
        .map(|node| node_text(code, node))
        .transpose()?;
    if c_return_type.as_deref().map(str::trim) != Some("int") {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "C procedure must return int (the mudu_proc_fn convention)",
            name.clone()
        ));
    }
    Ok(CProcedure {
        name,
        params,
        return_type,
        return_value_type,
    })
}

fn is_static_function(code: &str, function: Node) -> RS<bool> {
    let mut cursor = function.walk();
    for child in function.children(&mut cursor) {
        if child.kind() == "storage_class_specifier" && node_text(code, child)?.trim() == "static" {
            return Ok(true);
        }
    }
    Ok(false)
}

fn node_text(code: &str, node: Node) -> RS<String> {
    node.utf8_text(code.as_bytes())
        .map(|text| text.to_string())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode C source text error", e))
}

fn validate_procedure_signature(procedure: &CProcedure) -> RS<()> {
    if !is_valid_identifier(&procedure.name)
        || procedure
            .params
            .iter()
            .any(|param| !is_valid_identifier(&param.name))
    {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid C procedure identifier",
            procedure.name.clone()
        ));
    }
    Ok(())
}

#[cfg(all(test, not(miri)))]
#[path = "parser_test.rs"]
mod parser_test;
