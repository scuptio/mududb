//! Entry points for parsing standard and custom SQL statements.

use super::context::ParseContext;
use super::partition::{
    parse_partition_placement_item, parse_range_partition_def, parse_table_partition_suffix,
};
use super::utils::{
    contains_ignore_ascii_case, find_keyword_position, find_matching_paren, split_top_level_csv,
    starts_with_ignore_ascii_case,
};
use super::SQLParser;
use crate::ast::stmt_create_fs_type::{FsTypeKind, StmtCreateFsType};
use crate::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use crate::ast::stmt_create_partition_rule::StmtCreatePartitionRule;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_drop_type::StmtDropType;
use crate::ast::stmt_list::StmtList;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ts_const::{ts_field_name, ts_kind_id};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::Node;

impl SQLParser {
    /// Parse a SQL string using the standard tree-sitter grammar.
    pub(crate) fn parse_standard(&self, sql: &str) -> RS<StmtList> {
        let parse_context = ParseContext::new(sql.to_string());
        let mut guard = self.parser.lock()?;
        let opt_tree = guard.parse(sql, None);
        let tree = match opt_tree {
            Some(tree) => tree,
            None => return Err(mudu_error!(ErrorCode::MlParse, "SQL parse error")),
        };
        let vec = self.visit_root(&parse_context, tree.root_node())?;
        let stmt = StmtList::new(vec);
        Ok(stmt)
    }

    /// Try to parse custom statement syntax that is not covered by the grammar.
    pub(crate) fn try_parse_custom_statement(&self, sql: &str) -> RS<Option<StmtList>> {
        let trimmed = sql.trim();
        if trimmed.is_empty() {
            return Ok(Some(StmtList::new(Vec::new())));
        }
        let normalized = trimmed.trim_end_matches(';').trim();
        if normalized.is_empty() {
            return Ok(Some(StmtList::new(Vec::new())));
        }

        if starts_with_ignore_ascii_case(normalized, "create partition rule ") {
            let stmt = self.parse_create_partition_rule_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreatePartitionRule(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "create partition placement ") {
            let stmt = self.parse_create_partition_placement_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreatePartitionPlacement(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "create table ")
            && contains_ignore_ascii_case(normalized, " partition by global rule ")
        {
            let stmt = self.parse_create_table_partitioned_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreateTable(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "create type filesystem ") {
            let stmt = self.parse_create_fs_type_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::CreateFsType(stmt),
            )])));
        }

        if starts_with_ignore_ascii_case(normalized, "drop type ")
            || normalized.eq_ignore_ascii_case("drop type")
        {
            let stmt = self.parse_drop_type_custom(normalized)?;
            return Ok(Some(StmtList::new(vec![StmtType::Command(
                StmtCommand::DropType(stmt),
            )])));
        }

        Ok(None)
    }

    /// Parse a script that mixes custom statements with standard SQL by
    /// splitting it top-level and parsing each statement with the custom
    /// parser first, falling back to the standard tree-sitter parser.
    pub(crate) fn parse_mixed_script(&self, sql: &str) -> RS<StmtList> {
        let mut stmts = Vec::new();
        for chunk in split_top_level_statements(sql) {
            let trimmed = chunk.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(stmt_list) = self.try_parse_custom_statement(trimmed)? {
                stmts.extend(stmt_list.into_stmts());
            } else {
                let stmt_list = self.parse_standard(trimmed)?;
                stmts.extend(stmt_list.into_stmts());
            }
        }
        Ok(StmtList::new(stmts))
    }

    /// Parse a `CREATE TABLE ... PARTITION BY GLOBAL RULE ...` statement.
    pub(crate) fn parse_create_table_partitioned_custom(&self, sql: &str) -> RS<StmtCreateTable> {
        let close_index = find_matching_paren(
            sql,
            sql.find('(').ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Parse,
                    "partitioned create table has no column list"
                )
            })?,
        )?;
        let base_sql = sql[..=close_index].trim();
        let suffix = sql[close_index + 1..].trim();

        let mut stmt = match self.parse_standard(base_sql)?.stmts().first() {
            Some(StmtType::Command(StmtCommand::CreateTable(stmt))) => stmt.clone(),
            _ => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "failed to parse base create table statement"
                ));
            }
        };
        let partition = parse_table_partition_suffix(suffix)?;
        stmt.set_partition(partition);
        Ok(stmt)
    }

    /// Parse a `CREATE PARTITION RULE ...` statement.
    pub(crate) fn parse_create_partition_rule_custom(
        &self,
        sql: &str,
    ) -> RS<StmtCreatePartitionRule> {
        let prefix = "create partition rule ";
        let rest = sql[prefix.len()..].trim();
        let range_pos = find_keyword_position(rest, "range").ok_or_else(|| {
            mudu_error!(ErrorCode::Parse, "create partition rule must contain RANGE")
        })?;
        let rule_name = rest[..range_pos].trim();
        if rule_name.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "partition rule name is empty"
            ));
        }

        let range_body = rest[range_pos + "range".len()..].trim();
        if !range_body.starts_with('(') {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "partition rule RANGE clause must be wrapped in parentheses"
            ));
        }
        let close_index = find_matching_paren(range_body, 0)?;
        let inner = range_body[1..close_index].trim();
        let defs = split_top_level_csv(inner);
        let mut partitions = Vec::with_capacity(defs.len());
        for def in defs {
            partitions.push(parse_range_partition_def(def)?);
        }
        Ok(StmtCreatePartitionRule::new(
            rule_name.to_string(),
            partitions,
        ))
    }

    /// Parse a `CREATE PARTITION PLACEMENT ...` statement.
    pub(crate) fn parse_create_partition_placement_custom(
        &self,
        sql: &str,
    ) -> RS<StmtCreatePartitionPlacement> {
        let prefix = "create partition placement ";
        let rest = sql[prefix.len()..].trim();
        let for_rule_prefix = "for rule ";
        if !starts_with_ignore_ascii_case(rest, for_rule_prefix) {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "create partition placement must use FOR RULE"
            ));
        }
        let rest = rest[for_rule_prefix.len()..].trim();
        let open_index = rest.find('(').ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                "create partition placement must contain placement list"
            )
        })?;
        let close_index = find_matching_paren(rest, open_index)?;
        let rule_name = rest[..open_index].trim();
        let inner = &rest[open_index + 1..close_index];
        let placements = split_top_level_csv(inner)
            .into_iter()
            .map(parse_partition_placement_item)
            .collect::<RS<Vec<_>>>()?;
        if rule_name.is_empty() || placements.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "invalid create partition placement statement"
            ));
        }
        Ok(StmtCreatePartitionPlacement::new(
            rule_name.to_string(),
            placements,
        ))
    }

    /// Parse a `CREATE TYPE FILESYSTEM FILE|DIRECTORY <name>` statement.
    pub(crate) fn parse_create_fs_type_custom(&self, sql: &str) -> RS<StmtCreateFsType> {
        let prefix = "create type filesystem ";
        let rest = sql[prefix.len()..].trim();
        let (keyword, name) = match rest.find(char::is_whitespace) {
            Some(index) => (&rest[..index], rest[index..].trim()),
            None => (rest, ""),
        };
        let kind = if keyword.eq_ignore_ascii_case("file") {
            FsTypeKind::File
        } else if keyword.eq_ignore_ascii_case("directory") {
            FsTypeKind::Directory
        } else {
            return Err(mudu_error!(
                ErrorCode::Parse,
                "create type filesystem must specify FILE or DIRECTORY"
            ));
        };
        validate_type_name(name)?;
        Ok(StmtCreateFsType::new(name.to_string(), kind))
    }

    /// Parse a `DROP TYPE <name>` statement.
    pub(crate) fn parse_drop_type_custom(&self, sql: &str) -> RS<StmtDropType> {
        let prefix = "drop type";
        let name = sql[prefix.len()..].trim();
        validate_type_name(name)?;
        Ok(StmtDropType::new(name.to_string()))
    }

    /// Print a human-readable parse error if the node contains errors.
    pub(crate) fn parse_error(&self, context: &ParseContext, node: &Node) -> RS<()> {
        if node.has_error() {
            let mut buffer = Vec::new();
            super::error::print_parse_error(context.parse_str(), node, &mut buffer)?;
            let error = String::from_utf8(buffer)
                .map_err(|e| mudu_error!(ErrorCode::InvalidUtf8, "", e))?;
            Err(mudu_error!(
                ErrorCode::MlParse,
                format!(
                    "Syntax error at position start {}, end {}, at text\n\
                 \"\n\
                 {}\n\",\
                 \nErrors, {}",
                    node.start_position(),
                    node.end_position(),
                    super::error::ts_node_context_string(context.parse_str(), node)?,
                    error
                )
            ))
        } else {
            Ok(())
        }
    }

    /// Alias for [`Self::parse_error`].
    pub(crate) fn sql_parse_error(&self, context: &ParseContext, node: &Node) -> RS<()> {
        self.parse_error(context, node)
    }

    /// Visit the root program node and return the list of statements.
    pub(crate) fn visit_root(&self, context: &ParseContext, node: Node) -> RS<Vec<StmtType>> {
        self.sql_parse_error(context, &node)?;
        let mut vec = vec![];
        for i in 0..node.child_count() {
            let Some(child) = node.child(i as _) else {
                continue;
            };
            self.sql_parse_error(context, &child)?;
            if child.kind_id() == ts_kind_id::STATEMENT_TRANSACTION {
                let stmt = self.visit_transaction_statement(context, child)?;
                vec.push(stmt);
            }
        }
        Ok(vec)
    }

    /// Visit a transaction statement node.
    pub(crate) fn visit_transaction_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtType> {
        let _opt_node = node.child_by_field_name(ts_field_name::STATEMENT);
        let c = match node.child(0) {
            Some(c) => c,
            None => {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "no child in transaction statement"
                ));
            }
        };
        if c.kind_id() == ts_kind_id::STATEMENT {
            self.visit_statement(context, c)
        } else {
            Err(mudu_error!(
                ErrorCode::NotImplemented,
                "unsupported transaction statement"
            ))
        }
    }

    /// Visit a single statement node.
    pub(crate) fn visit_statement(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_stmt = node.child_by_field_name(ts_field_name::STMT_GUT);
        let d_stmt = match opt_stmt {
            Some(s) => s,
            None => {
                return Err(mudu_error!(ErrorCode::Parse, "no child in statement"));
            }
        };
        let stmt = self.visit_statement_gut(context, d_stmt)?;
        Ok(stmt)
    }
}

/// Validate that a type name is a non-empty identifier of alphanumeric
/// characters or underscores that does not start with a digit.
pub(crate) fn validate_type_name(name: &str) -> RS<()> {
    let valid = !name.is_empty()
        && !name.as_bytes()[0].is_ascii_digit()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_');
    if !valid {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!("invalid type name {}", name)
        ));
    }
    Ok(())
}

#[cfg(all(test, not(miri)))]
#[path = "entry_test.rs"]
mod entry_test;

/// True when the SQL text contains syntax only the custom parser handles
/// (partition DDL, filesystem types, or partitioned `CREATE TABLE`).
pub(crate) fn contains_custom_statement_syntax(sql: &str) -> bool {
    let lowered = sql.to_lowercase();
    lowered.contains("create partition rule ")
        || lowered.contains("create partition placement ")
        || lowered.contains("partition by global rule ")
        || lowered.contains("create type filesystem ")
}

/// Split a SQL script into top-level statements on `;` boundaries, skipping
/// `--` line comments and respecting single/double-quoted strings.
pub(crate) fn split_top_level_statements(sql: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    while let Some(ch) = chars.next() {
        if !in_single_quote && !in_double_quote && ch == '-' && chars.peek() == Some(&'-') {
            // Line comment: skip through the end of the line.
            for skipped in chars.by_ref() {
                if skipped == '\n' {
                    break;
                }
            }
            continue;
        }
        match ch {
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ';' if !in_single_quote && !in_double_quote => {
                statements.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    if !current.trim().is_empty() {
        statements.push(current);
    }
    statements
}
