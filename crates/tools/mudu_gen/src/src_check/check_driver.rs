//! The `check-sql` driver: load the DDL schema, extract SQL literals from
//! the input sources, run the core checks plus the call-site enhancements,
//! and render diagnostics as `file:line:col: severity: message` lines.

use crate::src_check::extract_as::extract_as_sql;
use crate::src_check::extract_c::extract_c_sql;
use crate::src_check::extract_cs::extract_cs_sql;
use crate::src_check::extract_go::extract_go_sql;
use crate::src_check::extract_py::extract_py_sql;
use crate::src_check::extract_rust::extract_rust_sql;
use crate::src_check::sql_literal::ExtractedSql;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_snake_case;
use mudu_binding::table::table_def::TableDef;
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::StmtType;
use sql_parser::check::checker::{check_sql, count_params, find_table, select_result_shape};
use sql_parser::check::diagnostic::SqlSeverity;
use sql_parser::check::param_type::{
    param_target_columns, param_type_compatible, uni_data_type_name,
};
use sql_parser::parser::ddl_parser::DDLParser;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// Source language of the files to check.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckLang {
    /// Rust guest sources (`.rs`).
    Rust,
    /// AssemblyScript guest sources (`.ts`).
    AssemblyScript,
    /// C# guest sources (`.cs`).
    CSharp,
    /// Python guest sources (`.py`).
    Python,
    /// C guest sources (`.c`; headers are not scanned).
    C,
    /// Go guest sources (`.go`).
    Go,
}

impl CheckLang {
    /// Resolve a `--lang` value (`rust`, `as`, `cs`, `py`, `c`, `go`; the
    /// aliases match the mpm-crate `--lang` naming).
    pub fn from_name(name: &str) -> Option<CheckLang> {
        match name.to_ascii_lowercase().as_str() {
            "rust" => Some(CheckLang::Rust),
            "as" | "assemblyscript" => Some(CheckLang::AssemblyScript),
            "cs" | "csharp" => Some(CheckLang::CSharp),
            "python" | "py" => Some(CheckLang::Python),
            "c" | "cc" | "cpp" => Some(CheckLang::C),
            "go" | "golang" => Some(CheckLang::Go),
            _ => None,
        }
    }

    fn extension(&self) -> &'static str {
        match self {
            CheckLang::Rust => "rs",
            CheckLang::AssemblyScript => "ts",
            CheckLang::CSharp => "cs",
            CheckLang::Python => "py",
            CheckLang::C => "c",
            CheckLang::Go => "go",
        }
    }

    fn extract(&self, source: &str) -> RS<Vec<ExtractedSql>> {
        match self {
            CheckLang::Rust => extract_rust_sql(source),
            CheckLang::AssemblyScript => extract_as_sql(source),
            CheckLang::CSharp => extract_cs_sql(source),
            CheckLang::Python => extract_py_sql(source),
            CheckLang::C => extract_c_sql(source),
            CheckLang::Go => extract_go_sql(source),
        }
    }
}

/// One diagnostic rendered for display, tagged with its severity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RenderedDiagnostic {
    /// The `file:line:col: severity: message` line.
    pub rendered: String,
    /// The diagnostic severity.
    pub severity: SqlSeverity,
}

/// Aggregated result of one `check-sql` run.
#[derive(Clone, Debug, Default)]
pub struct CheckOutcome {
    /// All diagnostics, in file order.
    pub diagnostics: Vec<RenderedDiagnostic>,
    /// Number of source files scanned.
    pub file_count: usize,
    /// Number of SQL literals checked.
    pub literal_count: usize,
    /// Number of [`SqlSeverity::Error`] diagnostics.
    pub error_count: usize,
    /// Number of [`SqlSeverity::Uncovered`] diagnostics.
    pub uncovered_count: usize,
}

/// Run the SQL static check.
///
/// - `lang`: `rust`, `as`, `cs`, `py`, `c`, or `go`;
/// - `ddl_paths`: one or more DDL SQL files whose `CREATE TABLE` statements
///   define the schema;
/// - `input`: a source file or a directory scanned recursively for files
///   with the language's extension.
pub fn run_check_sql(lang: &str, ddl_paths: &[String], input: &str) -> RS<CheckOutcome> {
    let check_lang = CheckLang::from_name(lang)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, format!("unknown lang: {lang}")))?;
    let schema = load_schema(ddl_paths)?;
    let files = collect_input_files(Path::new(input), check_lang.extension())?;

    let mut outcome = CheckOutcome::default();
    for file in &files {
        outcome.file_count += 1;
        let source = mudu_sys::fs::sync::sync_read_to_string(file)?;
        let extracted = check_lang.extract(&source)?;
        outcome.literal_count += extracted.len();
        let sql_parser = SQLParser::new()?;
        for item in &extracted {
            check_one_literal(item, &schema, &sql_parser, file, &mut outcome);
        }
    }
    Ok(outcome)
}

fn load_schema(ddl_paths: &[String]) -> RS<Vec<TableDef>> {
    let parser = DDLParser::new()?;
    let mut schema = Vec::new();
    for path in ddl_paths {
        let text = mudu_sys::fs::sync::sync_read_to_string(Path::new(path))?;
        for table in parser.parse(&text)? {
            merge_table(&mut schema, table);
        }
    }
    Ok(schema)
}

/// Merge one table into the schema. Deployment variants may define the
/// same table with different column sets (e.g. a partitioned variant adds
/// the partition key): union the columns so SQL written for either variant
/// checks cleanly. Columns keep first-occurrence order.
fn merge_table(schema: &mut Vec<TableDef>, table: TableDef) {
    let opt_existing = schema
        .iter_mut()
        .find(|def| def.table_name().eq_ignore_ascii_case(table.table_name()));
    match opt_existing {
        Some(existing) => {
            let mut columns = existing.table_columns().clone();
            for column in table.table_columns() {
                let known = columns
                    .iter()
                    .any(|c| c.column_name().eq_ignore_ascii_case(column.column_name()));
                if !known {
                    columns.push(column.clone());
                }
            }
            *existing = TableDef::new(existing.table_name().clone(), columns);
        }
        None => schema.push(table),
    }
}

fn collect_input_files(input: &Path, extension: &str) -> RS<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files_recursive(input, extension, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_files_recursive(path: &Path, extension: &str, files: &mut Vec<PathBuf>) -> RS<()> {
    let metadata = mudu_sys::fs::sync::sync_metadata(path)?;
    if metadata.is_dir() {
        for entry in mudu_sys::fs::sync::sync_read_dir_entries(path)? {
            collect_files_recursive(&entry.path(), extension, files)?;
        }
    } else if path.extension() == Some(OsStr::new(extension)) {
        files.push(path.to_path_buf());
    }
    Ok(())
}

fn check_one_literal(
    item: &ExtractedSql,
    schema: &[TableDef],
    sql_parser: &SQLParser,
    file: &Path,
    outcome: &mut CheckOutcome,
) {
    let file_name = file.to_string_lossy();
    // The vendored grammar cannot parse `:name` placeholders: rewrite them
    // to positional form first, then run every check on the rewritten SQL.
    // Positions in diagnostics then refer to the rewritten text (they drift
    // left of the original after the first rewritten name).
    let rewritten;
    let sql = match sql_parser::check::named_param::rewrite_named_params(&item.sql) {
        Ok(rewrite) if rewrite.has_named_params() => {
            rewritten = rewrite.positional_sql;
            rewritten.as_str()
        }
        Ok(_) => item.sql.as_str(),
        Err(e) => {
            // Mixing `?` and `:name` in one literal is an error.
            push_diagnostic(
                outcome,
                SqlSeverity::Error,
                format!(
                    "{}:{}:{}: error: {}",
                    file_name,
                    item.line,
                    item.column,
                    e.message()
                ),
            );
            return;
        }
    };
    for diagnostic in check_sql(sql, schema) {
        let (line, column) = map_position(item, diagnostic.line(), diagnostic.column());
        push_diagnostic(
            outcome,
            diagnostic.severity(),
            format!(
                "{}:{}:{}: {}: {}",
                file_name,
                line,
                column,
                diagnostic.severity().label(),
                diagnostic.message()
            ),
        );
    }
    enhance_call_site(item, sql, schema, sql_parser, &file_name, outcome);
}

/// Call-site enhancements: bind-parameter arity, `SELECT` result shape,
/// and literal parameter type checking. All require the SQL to parse;
/// unparseable or beyond-subset statements already carry diagnostics from
/// the core check. `sql` is the (possibly name-rewritten) statement text.
fn enhance_call_site(
    item: &ExtractedSql,
    sql: &str,
    schema: &[TableDef],
    sql_parser: &SQLParser,
    file_name: &str,
    outcome: &mut CheckOutcome,
) {
    if item.param_arity.is_none() && item.entity.is_none() && item.param_literal_tags.is_empty() {
        return;
    }
    let stmt_list = match sql_parser.parse(sql) {
        Ok(stmt_list) => stmt_list,
        Err(_) => return,
    };
    if let Some(arity) = item.param_arity {
        let expected: usize = stmt_list.stmts().iter().map(count_params).sum();
        if expected != arity {
            push_diagnostic(
                outcome,
                SqlSeverity::Error,
                format!(
                    "{}:{}:{}: error: the call supplies {arity} bind parameter(s) \
                     but the SQL has {expected} '?' placeholder(s)",
                    file_name, item.line, item.column
                ),
            );
        }
    }
    if !item.param_literal_tags.is_empty() {
        check_literal_param_types(item, sql, schema, file_name, outcome);
    }
    if let Some(entity) = &item.entity {
        check_result_shape(item, entity, schema, &stmt_list, file_name, outcome);
    }
}

/// Compare literal parameter elements (`42`, `"s"`, `1.5`, `true`) with
/// their target columns using the placeholder-to-column mapping and the
/// shared compatibility table. Non-literal elements and unmappable
/// placeholders are skipped.
fn check_literal_param_types(
    item: &ExtractedSql,
    sql: &str,
    schema: &[TableDef],
    file_name: &str,
    outcome: &mut CheckOutcome,
) {
    let columns = match param_target_columns(sql, schema) {
        Ok(columns) => columns,
        Err(_) => return,
    };
    for (index, (tag, column)) in item
        .param_literal_tags
        .iter()
        .zip(columns.iter())
        .enumerate()
    {
        let (Some(tag), Some(column)) = (tag, column) else {
            continue;
        };
        if !param_type_compatible(*tag, column.data_type()) {
            push_diagnostic(
                outcome,
                SqlSeverity::Error,
                format!(
                    "{}:{}:{}: error: parameter {index}: {} literal is not compatible \
                     with column '{}' of type {}",
                    file_name,
                    item.line,
                    item.column,
                    tag.label(),
                    column.column_name(),
                    uni_data_type_name(column.data_type())
                ),
            );
        }
    }
}

/// Compare a literal `SELECT`'s result shape with the column order of the
/// turbofish entity's table (`Wallets` → `wallets`). The check only runs
/// when the entity name maps to a known table; scalar turbofish types
/// (`mudu_query::<i64>`) are skipped.
fn check_result_shape(
    item: &ExtractedSql,
    entity: &str,
    schema: &[TableDef],
    stmt_list: &sql_parser::ast::stmt_list::StmtList,
    file_name: &str,
    outcome: &mut CheckOutcome,
) {
    let table_name = to_snake_case(entity);
    let Some(table_def) = find_table(schema, &table_name) else {
        return;
    };
    let stmts = stmt_list.stmts();
    let Some(StmtType::Select(select)) = stmts.first() else {
        return;
    };
    if stmts.len() != 1 {
        return;
    }
    let shape = select_result_shape(select, schema);
    let columns: Vec<&String> = table_def
        .table_columns()
        .iter()
        .map(|column| column.column_name())
        .collect();
    let matches = shape.len() == columns.len()
        && shape
            .iter()
            .zip(columns.iter())
            .all(|(selected, column)| selected.eq_ignore_ascii_case(column));
    if !matches {
        push_diagnostic(
            outcome,
            SqlSeverity::Error,
            format!(
                "{}:{}:{}: error: the SELECT result shape [{}] does not match \
                 the columns [{}] of entity table '{table_name}' ({entity})",
                file_name,
                item.line,
                item.column,
                shape.join(", "),
                columns
                    .iter()
                    .map(|c| c.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        );
    }
}

/// Map a position relative to the SQL text onto the source file: the
/// literal token position plus the relative offset. Line-1 columns point
/// into the literal token (quote included), later lines are exact.
fn map_position(item: &ExtractedSql, line: usize, column: usize) -> (usize, usize) {
    let file_line = item.line + line - 1;
    let file_column = if line == 1 {
        item.column + column - 1
    } else {
        column
    };
    (file_line, file_column)
}

fn push_diagnostic(outcome: &mut CheckOutcome, severity: SqlSeverity, rendered: String) {
    match severity {
        SqlSeverity::Error => outcome.error_count += 1,
        SqlSeverity::Uncovered => outcome.uncovered_count += 1,
    }
    outcome
        .diagnostics
        .push(RenderedDiagnostic { rendered, severity });
}
