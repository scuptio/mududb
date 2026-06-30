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
use crate::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use crate::ast::stmt_create_partition_rule::StmtCreatePartitionRule;
use crate::ast::stmt_create_table::StmtCreateTable;
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

        Ok(None)
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

#[cfg(all(test, not(miri)))]
#[path = "entry_test.rs"]
mod entry_test;
