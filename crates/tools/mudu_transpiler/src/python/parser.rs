//! Tree-sitter parser for the Python front-end.

use crate::common::ident::is_valid_identifier;
use crate::common::type_registry::TypeRegistry;
use crate::python::procedure::{PyParam, PyProcedure, PyValueType, normalize_hint};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::{Node, Parser, TreeCursor};

/// Discover all `# mudu-proc` marked procedures in `code`.
///
/// A procedure is a top-level `def` function immediately preceded by a
/// `# mudu-proc` comment line (any leading whitespace on the comment line,
/// and any amount of blank lines between the comment and the `def`).
///
/// `type_registry` carries the user-defined types loaded from `--type-wit`;
/// type hints resolve against the scalar table first and the registry
/// second (record/enum hints, and option forms of those).
pub fn discover_procedures(
    code: &str,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<PyProcedure>> {
    let mut parser = Parser::new();
    let language = tree_sitter_python::LANGUAGE;
    parser
        .set_language(&language.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load Python grammar error", e))?;
    let tree = parser
        .parse(code, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse Python source error"))?;
    let root = tree.root_node();
    if root.has_error() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "Python source has syntax error"
        ));
    }

    let mut comments = Vec::new();
    collect_comments(root, &mut comments);

    let mut procedures = Vec::new();
    let mut cursor = root.walk();
    for child in root.named_children(&mut cursor) {
        // For a decorated definition the marker comment precedes the first
        // decorator, so the marker check runs against the outer node while
        // the signature is parsed from the inner function definition.
        let (function, marked) = match child.kind() {
            "function_definition" => (child, child),
            "decorated_definition" => {
                let Some(definition) = child.child_by_field_name("definition") else {
                    continue;
                };
                if definition.kind() != "function_definition" {
                    continue;
                }
                (definition, child)
            }
            _ => continue,
        };
        if !has_mudu_proc_label(code, marked, &comments)? {
            continue;
        }
        let procedure = parse_function_definition(code, function, type_registry)?;
        validate_procedure_signature(&procedure)?;
        if procedures
            .iter()
            .any(|p: &PyProcedure| p.name == procedure.name)
        {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "duplicate Python procedure",
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

fn has_mudu_proc_label(code: &str, marked: Node, comments: &[Node]) -> RS<bool> {
    let Some(comment) = comments
        .iter()
        .rev()
        .find(|comment| comment.end_byte() <= marked.start_byte())
    else {
        return Ok(false);
    };
    if !node_text(code, *comment)?.contains("mudu-proc") {
        return Ok(false);
    }
    let between = &code[comment.end_byte()..marked.start_byte()];
    Ok(between.trim().is_empty())
}

fn parse_function_definition(
    code: &str,
    function: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<PyProcedure> {
    let name_node = function
        .child_by_field_name("name")
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing Python function name"))?;
    let name = node_text(code, name_node)?;

    let params_node = function.child_by_field_name("parameters").ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "missing Python parameter list",
            name.clone()
        )
    })?;
    let params = parse_parameters(code, params_node, type_registry)?;

    let return_type = function
        .child_by_field_name("return_type")
        .map(|node| node_text(code, node))
        .transpose()?;
    let return_value_types = parse_return_type(return_type.as_deref(), type_registry)?;
    let session_arg = detect_session_arg(&params);
    Ok(PyProcedure {
        name,
        params,
        session_arg,
        return_type,
        return_value_types,
    })
}

/// Detect the session convention: the first parameter is named `session` and
/// is either unannotated or annotated with an Oid hint (`UniOid` / `Oid`).
/// Such a parameter is bound from `UniProcedureParam.session` by the adapter
/// and excluded from the wire `param_list` and the desc fields (the same
/// convention as the AssemblyScript guest's leading `Oid` parameter).
///
/// Mixed conventions keep plain positional behavior: a `session` name in a
/// non-first position, or a first `session` parameter annotated with a
/// non-Oid hint (e.g. `session: int`), is treated as an ordinary value
/// parameter.
fn detect_session_arg(params: &[PyParam]) -> Option<String> {
    let first = params.first()?;
    if first.name == "session"
        && matches!(first.value_type, PyValueType::ObjectId | PyValueType::Any)
    {
        Some(first.name.clone())
    } else {
        None
    }
}

fn parse_parameters(
    code: &str,
    params_node: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<PyParam>> {
    let mut params = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.named_children(&mut cursor) {
        let param = match child.kind() {
            // `def f(x)`
            "identifier" => PyParam {
                name: node_text(code, child)?,
                ty: None,
                value_type: PyValueType::Any,
            },
            // `def f(x: hint)`
            "typed_parameter" => parse_typed_parameter(code, child, type_registry)?,
            // `def f(x=default)`
            "default_parameter" => {
                let name_node = child.child_by_field_name("name").ok_or_else(|| {
                    mudu_error!(ErrorCode::Parse, "missing Python parameter name")
                })?;
                PyParam {
                    name: node_text(code, name_node)?,
                    ty: None,
                    value_type: PyValueType::Any,
                }
            }
            // `def f(x: hint = default)`
            "typed_default_parameter" => {
                let name_node = child.child_by_field_name("name").ok_or_else(|| {
                    mudu_error!(ErrorCode::Parse, "missing Python parameter name")
                })?;
                let hint = child
                    .child_by_field_name("type")
                    .map(|node| node_text(code, node))
                    .transpose()?;
                PyParam {
                    name: node_text(code, name_node)?,
                    value_type: PyValueType::from_hint(hint.as_deref(), type_registry)?,
                    ty: hint,
                }
            }
            // `*args` / `**kwargs` cannot be mapped to the positional
            // `param_list` wire shape.
            "list_splat_pattern" | "dictionary_splat_pattern" => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "unsupported Python procedure parameter kind (splat)",
                    node_text(code, child)?
                ));
            }
            _ => continue,
        };
        params.push(param);
    }
    Ok(params)
}

fn parse_typed_parameter(
    code: &str,
    param_node: Node,
    type_registry: Option<&TypeRegistry>,
) -> RS<PyParam> {
    // The grammar gives `typed_parameter` a `type` field but no `name`
    // field; the name is its first identifier child.
    let name_node = param_node
        .child_by_field_name("name")
        .or_else(|| first_named_child_of_kind(param_node, "identifier"))
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "missing Python parameter name"))?;
    let hint = param_node
        .child_by_field_name("type")
        .map(|node| node_text(code, node))
        .transpose()?;
    Ok(PyParam {
        name: node_text(code, name_node)?,
        value_type: PyValueType::from_hint(hint.as_deref(), type_registry)?,
        ty: hint,
    })
}

/// Work out the statically obvious return arity: `-> None` yields no values,
/// `-> tuple[A, B, ...]` one value per element, anything else (including a
/// missing annotation) a single value.
///
/// Enum and option returns are rejected (v1: the wire result carries no
/// ordinal/nullable metadata for them — record and scalar returns only).
fn parse_return_type(
    return_type: Option<&str>,
    type_registry: Option<&TypeRegistry>,
) -> RS<Vec<PyValueType>> {
    let Some(return_type) = return_type else {
        return Ok(vec![PyValueType::Any]);
    };
    if normalize_hint(return_type) == "none" {
        return Ok(Vec::new());
    }
    let elements = match split_tuple_elements(return_type) {
        Some(elements) => elements,
        None => vec![return_type.to_string()],
    };
    let mut value_types = Vec::with_capacity(elements.len());
    for element in &elements {
        let value_type = PyValueType::from_hint(Some(element), type_registry)?;
        match &value_type {
            PyValueType::Enum(custom) => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "Python procedure result does not support the enum type '{}' (v1: enum parameters only)",
                        custom.name
                    )
                ));
            }
            PyValueType::Option(_) => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    format!(
                        "Python procedure result does not support the option type '{element}' (v1: option parameters only)"
                    )
                ));
            }
            _ => {}
        }
        value_types.push(value_type);
    }
    Ok(value_types)
}

/// Split the element list of a `tuple[...]` return annotation, tracking
/// bracket nesting so nested generics (e.g. `tuple[int, list[str]]`) split
/// only at the top level.
fn split_tuple_elements(return_type: &str) -> Option<Vec<String>> {
    let trimmed = return_type.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("tuple[") || !trimmed.ends_with(']') {
        return None;
    }
    let inner = &trimmed["tuple[".len()..trimmed.len() - 1];
    let mut elements = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                elements.push(inner[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if last.is_empty() {
        // `tuple[]` / trailing comma edge: arity is not statically obvious.
        return None;
    }
    elements.push(last.to_string());
    Some(elements)
}

fn first_named_child_of_kind<'tree>(node: Node<'tree>, kind: &str) -> Option<Node<'tree>> {
    let mut cursor: TreeCursor = node.walk();
    node.named_children(&mut cursor)
        .find(|child| child.kind() == kind)
}

fn node_text(code: &str, node: Node) -> RS<String> {
    node.utf8_text(code.as_bytes())
        .map(|text| text.to_string())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode Python source text error", e))
}

fn validate_procedure_signature(procedure: &PyProcedure) -> RS<()> {
    if !is_valid_identifier(&procedure.name)
        || procedure
            .params
            .iter()
            .any(|param| !is_valid_identifier(&param.name))
    {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "invalid Python procedure identifier",
            procedure.name.clone()
        ));
    }
    Ok(())
}

#[cfg(all(test, not(miri)))]
#[path = "parser_test.rs"]
mod parser_test;
