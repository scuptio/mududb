//! Rust SQL-literal extractor built on `syn`.
//!
//! All string literals in expression positions are filtered by the
//! universal rule ([`sql_literal::is_sql_candidate`]). Attribute literals
//! (including `#[doc = "..."]` doc comments) are skipped entirely.
//!
//! Two call-site enhancements are recorded:
//!
//! - when a literal appears in a call that also receives a parameter list
//!   (`sql_params!(&(a, b))`, `&(a, b)`, `&vec![..]`), its tuple arity is
//!   compared against the SQL `?` count by the driver;
//! - when the call carries a turbofish entity (`mudu_query::<Wallets>` or
//!   helpers such as `query_one_entity::<Customer>`) and the literal is a
//!   `SELECT`, the result shape is compared against the entity's table
//!   columns by the driver.

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use sql_parser::check::param_type::ParamTypeTag;
use std::collections::HashMap;
use syn::parse::Parser;
use syn::visit::Visit;
use syn::{Expr, ExprCall, ExprMacro, LitStr};

/// Extract SQL literals from Rust source code.
///
/// Returns an error when the source does not parse as Rust.
pub fn extract_rust_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let file = syn::parse_file(source)
        .map_err(|e| mudu_error!(ErrorCode::Parse, "parse Rust source error", e))?;
    let mut visitor = RustSqlVisitor::default();
    visitor.visit_file(&file);
    Ok(visitor.into_items())
}

type SpanKey = (usize, usize);

#[derive(Default)]
struct RustSqlVisitor {
    /// Insertion-ordered results; `index_by_span` deduplicates by the
    /// literal token position.
    items: Vec<ExtractedSql>,
    index_by_span: HashMap<SpanKey, usize>,
    /// Call context observed before the literal itself was visited
    /// (a call is visited before its arguments).
    pending_context: HashMap<SpanKey, OwnedContext>,
}

#[derive(Default)]
struct OwnedContext {
    param_arity: Option<usize>,
    entity: Option<String>,
    param_literal_tags: Vec<Option<ParamTypeTag>>,
}

impl RustSqlVisitor {
    fn into_items(self) -> Vec<ExtractedSql> {
        self.items
    }

    fn record_literal(&mut self, lit: &LitStr) {
        let value = lit.value();
        if !is_sql_candidate(&value) {
            return;
        }
        let key = span_key(lit);
        if self.index_by_span.contains_key(&key) {
            return;
        }
        let mut extracted = ExtractedSql::new(value, key.0, key.1);
        if let Some(context) = self.pending_context.remove(&key) {
            extracted.param_arity = context.param_arity;
            extracted.entity = context.entity;
            extracted.param_literal_tags = context.param_literal_tags;
        }
        self.index_by_span.insert(key, self.items.len());
        self.items.push(extracted);
    }

    fn record_context(&mut self, lit: &LitStr, context: OwnedContext) {
        let key = span_key(lit);
        match self.index_by_span.get(&key) {
            Some(index) => {
                let item = &mut self.items[*index];
                item.param_arity = context.param_arity;
                item.entity = context.entity;
                item.param_literal_tags = context.param_literal_tags;
            }
            None => {
                self.pending_context.insert(key, context);
            }
        }
    }

    fn analyze_call(&mut self, call: &ExprCall) {
        let mut literal: Option<LitStr> = None;
        let mut arity: Option<usize> = None;
        let mut literal_tags: Vec<Option<ParamTypeTag>> = Vec::new();
        for arg in &call.args {
            if literal.is_none()
                && let Some(lit) = sql_litstr_from_expr(arg)
                && is_sql_candidate(&lit.value())
            {
                literal = Some(lit);
            }
            if arity.is_none() {
                arity = params_arity_from_expr(arg);
            }
            if literal_tags.is_empty()
                && let Some(tags) = literal_tags_from_params_expr(arg)
            {
                literal_tags = tags;
            }
        }
        let Some(lit) = literal else {
            return;
        };
        let entity = turbofish_entity(&call.func);
        if arity.is_none() && entity.is_none() && literal_tags.is_empty() {
            return;
        }
        self.record_context(
            &lit,
            OwnedContext {
                param_arity: arity,
                entity,
                param_literal_tags: literal_tags,
            },
        );
    }
}

impl<'ast> Visit<'ast> for RustSqlVisitor {
    /// Skip attributes entirely: doc comments (`#[doc = "..."]`) and other
    /// attribute literals are never SQL.
    fn visit_attribute(&mut self, _attr: &'ast syn::Attribute) {}

    fn visit_lit_str(&mut self, lit: &'ast LitStr) {
        self.record_literal(lit);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        self.analyze_call(call);
        syn::visit::visit_expr_call(self, call);
    }

    fn visit_expr_macro(&mut self, expr_macro: &'ast ExprMacro) {
        // `sql_stmt!` tokens are opaque to the normal visit: unwrap the
        // macro so a literal inside it is still checked even when the
        // macro does not sit inside a call expression.
        if macro_name_is(expr_macro, "sql_stmt")
            && let Ok(expr) = syn::parse2::<Expr>(expr_macro.mac.tokens.clone())
            && let Some(lit) = sql_litstr_from_expr(&expr)
        {
            self.record_literal(&lit);
        }
        syn::visit::visit_expr_macro(self, expr_macro);
    }
}

fn span_key(lit: &LitStr) -> SpanKey {
    let start = lit.span().start();
    (start.line, start.column + 1)
}

fn macro_name_is(expr_macro: &ExprMacro, name: &str) -> bool {
    expr_macro
        .mac
        .path
        .segments
        .last()
        .map(|segment| segment.ident == name)
        .unwrap_or(false)
}

/// Unwrap the expression forms that carry a string literal to a query
/// call: plain literals, references, `to_string()`, and `sql_stmt!(..)`.
fn sql_litstr_from_expr(expr: &Expr) -> Option<LitStr> {
    match expr {
        Expr::Lit(expr_lit) => match &expr_lit.lit {
            syn::Lit::Str(lit) => Some(lit.clone()),
            _ => None,
        },
        Expr::Reference(reference) => sql_litstr_from_expr(&reference.expr),
        Expr::MethodCall(method) => {
            if method.method == "to_string" && method.args.is_empty() {
                sql_litstr_from_expr(&method.receiver)
            } else {
                None
            }
        }
        Expr::Macro(expr_macro) => {
            if macro_name_is(expr_macro, "sql_stmt") {
                let inner = syn::parse2::<Expr>(expr_macro.mac.tokens.clone()).ok()?;
                sql_litstr_from_expr(&inner)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Statically count the bind parameters of an argument expression, when it
/// has one of the known shapes: `sql_params!(&(..))`, `&(..)`, `&(x)`,
/// `&vec![..]`. Anything else (variables, method calls such as
/// `wallet.insert_params()`) is not statically known.
fn params_arity_from_expr(expr: &Expr) -> Option<usize> {
    match expr {
        Expr::Reference(reference) => params_arity_from_expr(&reference.expr),
        Expr::Tuple(tuple) => Some(tuple.elems.len()),
        Expr::Paren(paren) => params_arity_from_expr(&paren.expr).or(Some(1)),
        Expr::Macro(expr_macro) => {
            if macro_name_is(expr_macro, "sql_params") {
                let inner = syn::parse2::<Expr>(expr_macro.mac.tokens.clone()).ok()?;
                params_arity_from_expr(&inner)
            } else if macro_name_is(expr_macro, "vec") {
                if expr_macro.mac.tokens.is_empty() {
                    return Some(0);
                }
                let parser = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated;
                match parser.parse2(expr_macro.mac.tokens.clone()) {
                    Ok(list) if !list.is_empty() => Some(list.len()),
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Per-position type tags of the parameter tuple elements that are source
/// literals, for the same expression shapes [`params_arity_from_expr`]
/// knows. Returns `None` when the shape is not statically known.
fn literal_tags_from_params_expr(expr: &Expr) -> Option<Vec<Option<ParamTypeTag>>> {
    match expr {
        Expr::Reference(reference) => literal_tags_from_params_expr(&reference.expr),
        Expr::Tuple(tuple) => Some(tuple.elems.iter().map(literal_tag_of_expr).collect()),
        Expr::Paren(paren) => Some(vec![literal_tag_of_expr(&paren.expr)]),
        Expr::Macro(expr_macro) => {
            if macro_name_is(expr_macro, "sql_params") {
                let inner = syn::parse2::<Expr>(expr_macro.mac.tokens.clone()).ok()?;
                literal_tags_from_params_expr(&inner)
            } else if macro_name_is(expr_macro, "vec") {
                if expr_macro.mac.tokens.is_empty() {
                    return Some(Vec::new());
                }
                let parser = syn::punctuated::Punctuated::<Expr, syn::Token![,]>::parse_terminated;
                match parser.parse2(expr_macro.mac.tokens.clone()) {
                    Ok(list) if !list.is_empty() => {
                        Some(list.iter().map(literal_tag_of_expr).collect())
                    }
                    _ => None,
                }
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Tag one parameter element when it is a source literal (`42`, `0L`,
/// `"s"`, `1.5`, `true`, `b".."`, `'c'`, or their negated forms);
/// anything else (paths, calls, method receivers) is not statically
/// typed.
fn literal_tag_of_expr(expr: &Expr) -> Option<ParamTypeTag> {
    match expr {
        Expr::Lit(expr_lit) => literal_tag_of_lit(&expr_lit.lit),
        Expr::Unary(unary) => {
            if matches!(unary.op, syn::UnOp::Neg(_))
                && let Expr::Lit(expr_lit) = unary.expr.as_ref()
            {
                return literal_tag_of_lit(&expr_lit.lit);
            }
            None
        }
        _ => None,
    }
}

fn literal_tag_of_lit(lit: &syn::Lit) -> Option<ParamTypeTag> {
    match lit {
        syn::Lit::Int(_) => Some(ParamTypeTag::Integer),
        syn::Lit::Float(_) => Some(ParamTypeTag::Float),
        syn::Lit::Str(_) | syn::Lit::Char(_) => Some(ParamTypeTag::Text),
        syn::Lit::ByteStr(_) | syn::Lit::Byte(_) => Some(ParamTypeTag::Blob),
        syn::Lit::Bool(_) => Some(ParamTypeTag::Boolean),
        _ => None,
    }
}

/// Return the turbofish type name of a call (`mudu_query::<Wallets>` →
/// `Wallets`) when it is a single-segment type path.
fn turbofish_entity(func: &Expr) -> Option<String> {
    let Expr::Path(expr_path) = func else {
        return None;
    };
    let segment = expr_path.path.segments.last()?;
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return None;
    };
    for argument in &arguments.args {
        if let syn::GenericArgument::Type(syn::Type::Path(type_path)) = argument
            && type_path.qself.is_none()
            && type_path.path.segments.len() == 1
        {
            let segment = type_path.path.segments.first()?;
            if segment.arguments.is_empty() {
                return Some(segment.ident.to_string());
            }
        }
    }
    None
}
