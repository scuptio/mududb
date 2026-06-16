use mudu::common::result_of::{rs_of_opt, rs_option};

use mudu::common::result::RS;
use mudu::error::ec::EC;

use crate::ast::column_def::ColumnDef;
use crate::ast::expr_arithmetic::ExprArithmetic;
use crate::ast::expr_compare::ExprCompare;
use crate::ast::expr_item::{ExprItem, ExprValue};
use crate::ast::expr_literal::ExprLiteral;
use crate::ast::expr_logical::ExprLogical;
use crate::ast::expr_name::ExprName;
use crate::ast::expr_operator::Operator;
use crate::ast::expr_visitor::ExprVisitor;
use crate::ast::expression::ExprType;
use crate::ast::select_term::SelectTerm;
use crate::ast::stmt_copy_from::StmtCopyFrom;
use crate::ast::stmt_copy_to::StmtCopyTo;
use crate::ast::stmt_create_partition_placement::{
    StmtCreatePartitionPlacement, StmtPartitionPlacementItem,
};
use crate::ast::stmt_create_partition_rule::{
    StmtCreatePartitionRule, StmtPartitionBound, StmtRangePartition,
};
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ast::stmt_insert::StmtInsert;
use crate::ast::stmt_list::StmtList;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_table_partition::StmtTablePartition;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ast::stmt_update::{AssignedValue, Assignment, StmtUpdate};
use crate::ts_const::{ts_field_name, ts_kind_id, ts_kind_name};
use mudu::common::id::AttrIndex;
use mudu::data_type::numeric::Numeric;
use mudu::error::err::MError;
use mudu::m_error;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_dat_value::UniDatValue;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_scalar_value::UniScalarValue;
use mudu_sys::sync::SMutex;
use mudu_type::dat_typed::DatTyped;
use std::collections::HashMap;
use std::io::Write;
use std::str::FromStr;
use std::sync::Arc;
use substring::Substring;
use tree_sitter::{Language, Node, Parser};
use tree_sitter_sql;

pub struct SQLParser {
    parser: SMutex<Parser>,
}

struct ParseContext {
    text: String,
}

impl ParseContext {
    fn new(text: String) -> Self {
        Self { text }
    }

    fn parse_str(&self) -> &str {
        self.text.as_str()
    }
}

fn sql_language() -> Language {
    tree_sitter_sql::LANGUAGE.into()
}

fn traverse_tree_for_error_nodes<'t>(node: &Node<'t>, error_nodes: &mut Vec<Node<'t>>) {
    if !node.has_error() {
        return;
    }

    if node.kind() == "ERROR" || node.is_missing() {
        error_nodes.push(*node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree_for_error_nodes(&child, error_nodes);
    }
}

fn node_or_descendant_has_kind(node: Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| node_or_descendant_has_kind(child, kind));
    found
}

fn error_text(
    parse_text: &str,
    line_start: usize,
    column_start: usize,
    line_end: usize,
    column_end: usize,
) -> RS<String> {
    let line_start = line_start - 1;
    let column_start = column_start - 1;
    let line_end = line_end - 1;
    let column_end = column_end - 1;

    let mut err_text = String::new();
    let lines: Vec<_> = parse_text.lines().collect();
    for i in line_start..=line_end {
        let opt = lines.get(i);
        if let Some(s) = opt {
            let str = if i == line_start && i != line_end {
                s[column_start..].to_string()
            } else if i != line_start && i == line_end {
                s[..column_end].to_string()
            } else if i == line_start && i == line_end {
                s[column_start..column_end].to_string()
            } else {
                s.to_string()
            };
            err_text.push_str(&str);
        } else {
            err_text.clear();
            break;
        }
    }
    Ok(err_text)
}
fn print_error_line<W: Write>(parse_text: &str, node: Node, writter: &mut W) -> RS<()> {
    // row and column start at 0
    let line_start = node.start_position().row + 1;
    let line_end = node.end_position().row + 1;
    let column_start = node.start_position().column + 1;
    let column_end = node.end_position().column + 1;

    let mut cursor = node.walk();
    let mut tokens = String::new();

    for (i, child) in node.children(&mut cursor).enumerate() {
        let text = ts_node_context_string(parse_text, &child)?;
        if i != 0 {
            tokens.push_str(", ");
        }
        tokens.push_str(&text);
    }
    let kind = if let Some(parent) = node.parent() {
        parent.kind()
    } else {
        "root"
    };
    let error_text = error_text(parse_text, line_start, column_start, line_end, column_end)?;

    let error_msg = format!(
        "In \
        position: [{},{}; {},{}], \
        text: [{}]
        child tokens:[{}], \
        parent kind:[{}],\
        s-expr: [{}]\n",
        line_start,
        column_start,
        line_end,
        column_end,
        error_text,
        tokens,
        kind,
        node.to_sexp()
    );

    writter.write_fmt(format_args!("{}", error_msg)).unwrap();
    Ok(())
}

fn print_parse_error<W: Write>(parse_text: &str, node: &Node, writer: &mut W) -> RS<()> {
    let mut error_nodes = vec![];
    traverse_tree_for_error_nodes(node, &mut error_nodes);
    for node in error_nodes {
        print_error_line(parse_text, node, writer)?
    }
    Ok(())
}

impl Default for SQLParser {
    fn default() -> Self {
        Self::new()
    }
}

impl SQLParser {
    pub fn new() -> SQLParser {
        let mut parser = Parser::new();
        parser.set_language(&sql_language()).unwrap();
        Self {
            parser: SMutex::new(parser),
        }
    }

    pub fn parse(&self, sql: &str) -> RS<StmtList> {
        if let Some(stmt_list) = self.try_parse_custom_statement(sql)? {
            return Ok(stmt_list);
        }
        self.parse_standard(sql)
    }

    fn parse_standard(&self, sql: &str) -> RS<StmtList> {
        let parse_context = ParseContext::new(sql.to_string());
        let mut guard = self.parser.lock().unwrap();
        let opt_tree = guard.parse(sql, None);
        let tree = match opt_tree {
            Some(tree) => tree,
            None => return Err(m_error!(EC::MLParseError, "SQL parse error")),
        };
        let vec = self.visit_root(&parse_context, tree.root_node())?;
        let stmt = StmtList::new(vec);
        Ok(stmt)
    }

    fn try_parse_custom_statement(&self, sql: &str) -> RS<Option<StmtList>> {
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

    fn parse_create_table_partitioned_custom(&self, sql: &str) -> RS<StmtCreateTable> {
        let close_index = find_matching_paren(
            sql,
            sql.find('(').ok_or_else(|| {
                m_error!(EC::ParseErr, "partitioned create table has no column list")
            })?,
        )?;
        let base_sql = sql[..=close_index].trim();
        let suffix = sql[close_index + 1..].trim();

        let mut stmt = match self.parse_standard(base_sql)?.stmts().first() {
            Some(StmtType::Command(StmtCommand::CreateTable(stmt))) => stmt.clone(),
            _ => {
                return Err(m_error!(
                    EC::ParseErr,
                    "failed to parse base create table statement"
                ));
            }
        };
        let partition = parse_table_partition_suffix(suffix)?;
        stmt.set_partition(partition);
        Ok(stmt)
    }

    fn parse_create_partition_rule_custom(&self, sql: &str) -> RS<StmtCreatePartitionRule> {
        let prefix = "create partition rule ";
        let rest = sql[prefix.len()..].trim();
        let range_pos = find_keyword_position(rest, "range")
            .ok_or_else(|| m_error!(EC::ParseErr, "create partition rule must contain RANGE"))?;
        let rule_name = rest[..range_pos].trim();
        if rule_name.is_empty() {
            return Err(m_error!(EC::ParseErr, "partition rule name is empty"));
        }

        let range_body = rest[range_pos + "range".len()..].trim();
        if !range_body.starts_with('(') {
            return Err(m_error!(
                EC::ParseErr,
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

    fn parse_create_partition_placement_custom(
        &self,
        sql: &str,
    ) -> RS<StmtCreatePartitionPlacement> {
        let prefix = "create partition placement ";
        let rest = sql[prefix.len()..].trim();
        let for_rule_prefix = "for rule ";
        if !starts_with_ignore_ascii_case(rest, for_rule_prefix) {
            return Err(m_error!(
                EC::ParseErr,
                "create partition placement must use FOR RULE"
            ));
        }
        let rest = rest[for_rule_prefix.len()..].trim();
        let open_index = rest.find('(').ok_or_else(|| {
            m_error!(
                EC::ParseErr,
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
            return Err(m_error!(
                EC::ParseErr,
                "invalid create partition placement statement"
            ));
        }
        Ok(StmtCreatePartitionPlacement::new(
            rule_name.to_string(),
            placements,
        ))
    }

    fn parse_error(&self, context: &ParseContext, node: &Node) -> RS<()> {
        if node.has_error() {
            let mut buffer = Vec::new();
            print_parse_error(context.parse_str(), node, &mut buffer)?;
            let error = String::from_utf8(buffer).map_err(|e| m_error!(EC::MuduError, "", e))?;
            Err(m_error!(
                EC::MLParseError,
                format!(
                    "Syntax error at position start {}, end {}, at text\n\
                 \"\n\
                 {}\n\",\
                 \nErrors, {}",
                    node.start_position(),
                    node.end_position(),
                    ts_node_context_string(context.parse_str(), node)?,
                    error
                )
            ))
        } else {
            Ok(())
        }
    }

    fn sql_parse_error(&self, context: &ParseContext, node: &Node) -> RS<()> {
        self.parse_error(context, node)
    }

    fn visit_root(&self, context: &ParseContext, node: Node) -> RS<Vec<StmtType>> {
        self.sql_parse_error(context, &node)?;
        let mut vec = vec![];
        for i in 0..node.child_count() {
            let child = node.child(i as _).unwrap();
            self.sql_parse_error(context, &child)?;
            if child.kind_id() == ts_kind_id::STATEMENT_TRANSACTION {
                let stmt = self.visit_transaction_statement(context, child)?;
                vec.push(stmt);
            }
        }
        Ok(vec)
    }

    fn visit_transaction_statement(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let _opt_node = node.child_by_field_name(ts_field_name::STATEMENT);
        let c = match node.child(0) {
            Some(c) => c,
            None => {
                return Err(m_error!(EC::NoneErr, "no child in transaction statement"));
            }
        };
        if c.kind_id() == ts_kind_id::STATEMENT {
            self.visit_statement(context, c)
        } else {
            todo!()
        }
    }

    fn visit_statement(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_stmt = node.child_by_field_name(ts_field_name::STMT_GUT);
        let d_stmt = match opt_stmt {
            Some(s) => s,
            None => {
                return Err(m_error!(EC::NoneErr, "no child in statement"));
            }
        };
        let stmt = self.visit_statement_gut(context, d_stmt)?;
        Ok(stmt)
    }

    fn visit_statement_gut(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let kind = node.kind_id();
        match kind {
            ts_kind_id::DML_READ_STMT => self.visit_dml_read_stmt(context, node),
            ts_kind_id::DML_WRITE_STMT => self.visit_dml_write_stmt(context, node),
            ts_kind_id::DDL_STMT => self.visit_ddl_stmt(context, node),
            ts_kind_id::COPY_STMT => self.visit_copy_stmt(context, node),
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }

    fn visit_dml_read_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::SELECT_STATEMENT => {
                let stmt = self.visit_select_statement(context, child)?;
                Ok(StmtType::Select(stmt))
            }
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }

    fn visit_ddl_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();

        match kind {
            ts_kind_id::CREATE_TABLE_STATEMENT => {
                let stmt = self.visit_create_table_statement(context, child)?;
                Ok(StmtType::Command(StmtCommand::CreateTable(stmt)))
            }
            ts_kind_id::DROP_STATEMENT => {
                let stmt = self.visit_drop_statement(context, child)?;
                Ok(StmtType::Command(StmtCommand::DropTable(stmt)))
            }
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }

    fn visit_dml_write_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::INSERT_STATEMENT => {
                let stmt = self.visit_insert_statement(context, child)?;
                Ok(StmtType::Command(StmtCommand::Insert(stmt)))
            }
            ts_kind_id::UPDATE_STATEMENT => {
                let stmt = self.visit_update_statement(context, child)?;
                Ok(StmtType::Command(StmtCommand::Update(stmt)))
            }
            ts_kind_id::DELETE_STATEMENT => {
                let stmt = self.visit_delete_statement(context, child)?;
                Ok(StmtType::Command(StmtCommand::Delete(stmt)))
            }
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }

    fn visit_copy_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::COPY_FROM => self.visit_copy_from_stmt(context, child),
            ts_kind_id::COPY_TO => self.visit_copy_to_stmt(context, child),
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }

    fn visit_copy_from_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let n = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_obj_ref = rs_of_opt(n, || m_error!(EC::ParseErr, "no object reference field"))?;
        let table_name = self.visit_object_reference(context, n_obj_ref)?;
        let n = node.child_by_field_name(ts_field_name::FILE_PATH);
        let n_file_path = rs_of_opt(n, || m_error!(EC::ParseErr, "no object file path field"))?;
        let file_path = self.visit_string(context, n_file_path)?;
        let copy_from = StmtCopyFrom::new(file_path, table_name, vec![]);
        let st = StmtType::Command(StmtCommand::CopyFrom(copy_from));
        Ok(st)
    }

    fn visit_copy_to_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        // tree-sitter `copy_to` currently does not expose field names for children.
        let mut object_ref = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let mut file_path = node.child_by_field_name(ts_field_name::FILE_PATH);
        for i in 0..node.child_count() {
            let child = node.child(i as _).unwrap();
            if object_ref.is_none() && child.kind_id() == ts_kind_id::OBJECT_REFERENCE {
                object_ref = Some(child);
            } else if file_path.is_none() && child.kind_id() == ts_kind_id::FILE_PATH {
                file_path = Some(child);
            }
        }

        let n_obj_ref = rs_of_opt(object_ref, || {
            m_error!(EC::ParseErr, "no object reference field")
        })?;
        let table_name = self.visit_object_reference(context, n_obj_ref)?;
        let n_file_path = rs_of_opt(file_path, || {
            m_error!(EC::ParseErr, "no object file path field")
        })?;
        let file_path = self.visit_string(context, n_file_path)?;
        let copy_to = StmtCopyTo::new(file_path, table_name, vec![]);
        Ok(StmtType::Command(StmtCommand::CopyTo(copy_to)))
    }

    fn visit_drop_statement(&self, context: &ParseContext, node: Node) -> RS<StmtDropTable> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::DROP_TABLE => {
                let s = self.visit_drop_table_statement(context, child)?;
                Ok(s)
            }
            _ => Err(m_error!(EC::NotImplemented)),
        }
    }
    fn visit_drop_table_statement(&self, context: &ParseContext, node: Node) -> RS<StmtDropTable> {
        let opt = node.child_by_field_name(ts_field_name::IF_EXIST);
        let if_exist = opt.is_some();
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n = match opt {
            Some(n) => n,
            None => {
                return Err(m_error!(EC::NoneErr, "drop table statement"));
            }
        };
        let object = self.visit_object_reference(context, n)?;
        Ok(StmtDropTable::new(object, if_exist))
    }

    fn visit_select_statement(&self, context: &ParseContext, node: Node) -> RS<StmtSelect> {
        let mut stmt = StmtSelect::new();
        let opt_select = node.child_by_field_name(ts_field_name::SELECT);
        let select = match opt_select {
            Some(select) => select,
            None => {
                return Err(m_error!(EC::NoneErr, "no select statement"));
            }
        };
        let opt_from = node.child_by_field_name(ts_field_name::FROM);
        let from = match opt_from {
            Some(from) => from,
            None => {
                return Err(m_error!(EC::NoneErr, "no from field"));
            }
        };

        self.visit_select(context, select, &mut stmt)?;
        self.visit_from(context, from, &mut stmt)?;
        Ok(stmt)
    }

    fn visit_select(&self, context: &ParseContext, node: Node, stmt: &mut StmtSelect) -> RS<()> {
        let opt_select_expression = node.child_by_field_name(ts_field_name::SELECT_EXPRESSION);
        let select_expression = match opt_select_expression {
            Some(e) => e,
            None => {
                return Err(m_error!(EC::NoneErr, "no select expression"));
            }
        };

        self.visit_select_expression(context, select_expression, stmt)?;

        Ok(())
    }

    fn visit_from(&self, context: &ParseContext, node: Node, stmt: &mut StmtSelect) -> RS<()> {
        let opt_n_relation = node.child_by_field_name(ts_field_name::RELATION);
        let n_relation = rs_option(opt_n_relation, "")?;
        self.visit_relation(context, n_relation, stmt)?;
        let opt_n_where = node.child_by_field_name(ts_field_name::WHERE);
        if let Some(n_where) = opt_n_where {
            let where_predicate_list = self.visit_where(context, n_where)?;
            for p in where_predicate_list {
                stmt.add_where_predicate(p);
            }
        }

        Ok(())
    }

    fn visit_where(&self, context: &ParseContext, node: Node) -> RS<Vec<ExprCompare>> {
        let opt = node.child_by_field_name(ts_field_name::PREDICATE);
        let n_predicate = rs_option(opt, "")?;
        let where_predicate_list = self.visit_where_predicate_expression(context, n_predicate)?;
        Ok(where_predicate_list)
    }

    fn visit_where_predicate_expression(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<ExprCompare>> {
        let expr = self.visit_expression(context, node)?;
        let mut cmp_list = vec![];
        ExprVisitor::extract_expr_compare_list(expr, &mut cmp_list);
        Ok(cmp_list)
    }

    fn visit_expression(&self, context: &ParseContext, node: Node) -> RS<ExprType> {
        let opt_binary_expression = node.child_by_field_name(ts_field_name::BINARY_EXPRESSION);
        if let Some(n) = opt_binary_expression {
            return self.visit_binary_expression(context, n);
        }

        let opt_literal = node.child_by_field_name(ts_field_name::LITERAL);
        if let Some(n) = opt_literal {
            let literal = self.visit_literal(context, n)?;
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemValue(
                ExprValue::ValueLiteral(literal),
            ))));
        }

        let opt_qualified_field = node.child_by_field_name(ts_field_name::QUALIFIED_FIELD);
        if let Some(n) = opt_qualified_field {
            let field = self.visit_qualified_field(context, n)?;
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemName(field))));
        }

        let opt_expression = node.child_by_field_name(ts_field_name::EXPRESSION_IN_PARENTHESIS);
        if let Some(n) = opt_expression {
            return self.visit_expression(context, n);
        }

        let opt_place_holder = node.child_by_field_name(ts_field_name::PARAMETER_PLACEHOLDER);
        if let Some(_n) = opt_place_holder {
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemValue(
                ExprValue::ValuePlaceholder,
            ))));
        }
        Err(m_error!(
            EC::ParseErr,
            format!(
                "unknown expression {}",
                ts_node_context_string(context.parse_str(), &node)?
            )
        ))
    }

    fn visit_literal(&self, context: &ParseContext, node: Node) -> RS<ExprLiteral> {
        if node
            .child_by_field_name(ts_field_name::KEYWORD_NULL)
            .is_some()
            || node.kind() == ts_kind_name::S_KEYWORD_NULL
            || ts_node_context_string(context.parse_str(), &node)?.eq_ignore_ascii_case("null")
        {
            return Ok(ExprLiteral::Null);
        }
        let typed = if let Some(n) = node.child_by_field_name("integer") {
            let s = self.visit_integer(context, n)?;
            let i = i64::from_str(s.as_str()).unwrap();
            DatTyped::from_i64(i)
        } else if let Some(n) = node.child_by_field_name("decimal") {
            let s = self.visit_decimal(context, n)?;
            let numeric = Numeric::parse(s.as_str())
                .map_err(|e| m_error!(EC::ParseErr, format!("parse numeric error {}", e)))?;
            DatTyped::from_numeric(numeric)
        } else if let Some(n) = node.child_by_field_name("string") {
            let s = self.visit_string(context, n)?;
            DatTyped::from_string(s)
        } else if let Some(_n) = node.child_by_field_name("keyword_true") {
            todo!()
        } else if let Some(_n) = node.child_by_field_name("keyword_false") {
            todo!()
        } else {
            todo!()
        };
        Ok(ExprLiteral::DatumLiteral(typed))
    }

    fn visit_qualified_field(&self, context: &ParseContext, node: Node) -> RS<ExprName> {
        let opt = node.child_by_field_name(ts_field_name::IDENTIFIER_NAME);
        let n = rs_option(opt, "")?;
        let name = self.visit_identifier(context, n)?;
        let mut field = ExprName::new();
        field.set_name(name);
        Ok(field)
    }

    fn visit_binary_expression(&self, context: &ParseContext, node: Node) -> RS<ExprType> {
        let opt_n_operator = node.child_by_field_name(ts_field_name::OPERATOR);
        let n_operation = rs_option(opt_n_operator, "no operator in binary expression")?;
        let op = self.visit_operator(context, n_operation)?;
        let opt_left = node.child_by_field_name(ts_field_name::LEFT);
        let left = rs_option(opt_left, "no left in binary expression")?;
        let opt_right = node.child_by_field_name(ts_field_name::RIGHT);
        let right = rs_option(opt_right, "no right in binary expression")?;
        let expr_left = self.visit_expression(context, left)?;
        let expr_right = self.visit_expression(context, right)?;
        let expr: ExprType = match op {
            Operator::OValueCompare(c) => {
                let (l, r) = match (expr_left, expr_right) {
                    (ExprType::Value(l), ExprType::Value(r)) => ((*l).clone(), (*r).clone()),
                    _ => return Err(m_error!(EC::NotImplemented)),
                };
                ExprType::Compare(Arc::new(ExprCompare::new(c, l, r)))
            }
            Operator::OLogicalConnective(c) => {
                ExprType::Logical(Arc::new(ExprLogical::new(c, expr_left, expr_right)))
            }
            Operator::OArithmetic(c) => {
                ExprType::Arithmetic(Arc::new(ExprArithmetic::new(c, expr_left, expr_right)))
            }
        };

        Ok(expr)
    }

    fn visit_operator(&self, context: &ParseContext, node: Node) -> RS<Operator> {
        let op_string = ts_node_context_string(context.parse_str(), &node)?;
        Operator::from_name(op_string)
    }

    fn visit_relation(&self, context: &ParseContext, node: Node, stmt: &mut StmtSelect) -> RS<()> {
        let opt_n_object_reference = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference =
            rs_option(opt_n_object_reference, "no object reference in relation")?;
        let name = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(name);
        Ok(())
    }

    fn visit_object_reference(&self, context: &ParseContext, node: Node) -> RS<String> {
        let opt_n_object_name = node.child_by_field_name(ts_field_name::OBJECT_NAME);
        let n_object_name = rs_option(opt_n_object_name, "no object name in object reference")?;
        let name = ts_node_context_string(context.parse_str(), &n_object_name)?;
        Ok(name)
    }

    fn visit_select_expression(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtSelect,
    ) -> RS<()> {
        for i in 0..node.child_count() {
            let n = node.child(i as _).unwrap();
            if n.kind().eq("term") {
                let term = self.visit_term(context, n)?;
                stmt.add_select_term(term);
            }
        }

        Ok(())
    }

    fn visit_term(&self, context: &ParseContext, node: Node) -> RS<SelectTerm> {
        let mut term = SelectTerm::new();
        let opt_expression = node.child_by_field_name(ts_field_name::EXPRESSION);
        match opt_expression {
            Some(expression) => {
                self.visit_projection_expression(context, expression, &mut term)?;
                let opt_alias_name = node.child_by_field_name(ts_field_name::ALIAS);
                if let Some(alias) = opt_alias_name {
                    let alias = self.visit_alias_name(context, alias)?;
                    term.set_alias(alias);
                }
            }
            None => {
                let opt_all_fields = node.child_by_field_name(ts_field_name::ALL_FIELDS);
                match opt_all_fields {
                    Some(_) => {}
                    None => {
                        return Err(m_error!(EC::NoneErr, "no term found"));
                    }
                };
            }
        };
        Ok(term)
    }

    fn visit_projection_expression(
        &self,
        context: &ParseContext,
        node: Node,
        term: &mut SelectTerm,
    ) -> RS<()> {
        let opt_identifier = node.child_by_field_name(ts_field_name::QUALIFIED_FIELD);
        match opt_identifier {
            Some(n) => {
                let field = self.visit_qualified_field(context, n)?;
                term.set_field(field);
            }
            None => return Err(m_error!(EC::NotImplemented)),
        };
        Ok(())
    }

    fn visit_alias_name(&self, context: &ParseContext, node: Node) -> RS<String> {
        let opt_alias = node.child_by_field_name(ts_field_name::ALIAS);
        match opt_alias {
            None => Err(m_error!(
                EC::NoneErr,
                format!(
                    "alias not found in {}",
                    ts_node_context_string(context.parse_str(), &node)?
                )
            )),
            Some(n) => {
                let s = ts_node_context_string(context.parse_str(), &n)?;
                Ok(s)
            }
        }
    }

    fn visit_identifier(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    fn visit_string(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    fn visit_integer(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    fn visit_decimal(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    fn visit_create_table_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtCreateTable> {
        let opt_n_name = node.child_by_field_name(ts_field_name::TABLE_NAME);
        let n_name = rs_option(opt_n_name, "no table name in create table statement")?;
        let table_name = self.visit_identifier(context, n_name)?;
        let mut stmt_create_table = StmtCreateTable::new(table_name);
        let opt_n_cd = node.child_by_field_name(ts_field_name::COLUMN_DEFINITIONS);
        let n_cd = rs_option(opt_n_cd, "no column definitions in create table statement")?;
        self.visit_column_definitions(context, n_cd, &mut stmt_create_table)?;

        stmt_create_table.assign_index_for_columns();

        Ok(stmt_create_table)
    }

    fn visit_column_definitions(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let n = node.child_count();
        for i in 0..n {
            let c = node.child(i as _).unwrap();
            if c.kind_id() == ts_kind_id::COLUMN_DEFINITION {
                self.visit_column_definition(context, c, stmt)?;
            } else if c.kind_id() == ts_kind_id::CONSTRAINTS {
                self.visit_constraints(context, c, stmt)?;
            }
        }
        Ok(())
    }

    fn visit_constraints(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::CONSTRAINT, &mut cursor);
        for n in iter {
            self.visit_constraint(context, n, stmt)?;
        }

        Ok(())
    }

    fn visit_constraint(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        if let Some(n) = node.child_by_field_name(ts_field_name::PRIMARY_KEY_CONSTRAINT) {
            self.visit_primary_key_constraint(context, n, stmt)?;
        }

        Ok(())
    }

    fn visit_primary_key_constraint(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let opt_n = node.child_by_field_name(ts_field_name::COLUMN_LIST);

        let n = rs_option(opt_n, "no column list in primary key constraint")?;
        let mut map = HashMap::new();
        for d in stmt.mutable_column_def().iter_mut() {
            map.insert(d.column_name().clone(), d);
        }
        let mut index = 0;
        let mut f = |name: String| {
            if let Some(n) = map.get_mut(&name) {
                n.set_primary_key_index(Some(index));
                index += 1;
                Ok(())
            } else {
                Err(m_error!(EC::NoSuchElement))
            }
        };
        self.visit_column_list(context, n, &mut f)?;
        Ok(())
    }

    fn visit_column_definition(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let opt_n = node.child_by_field_name(ts_field_name::COLUMN_NAME);
        let n_column_name = rs_option(opt_n, "")?;
        let column_name = self.visit_identifier(context, n_column_name)?;

        let opt_n = node.child_by_field_name(ts_field_name::DATA_TYPE);
        let n_data_type = rs_option(opt_n, "")?;
        let (dat_type, opt_type_params) = self.visit_data_type(context, n_data_type)?;
        let mut column_def = ColumnDef::new(column_name, dat_type, opt_type_params);
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::COLUMN_CONSTRAINT, &mut cursor);
        let mut index_map = HashMap::new();
        for n in iter {
            self.visit_column_constraint(n, &mut column_def, &mut index_map)?;
        }

        stmt.add_column_def(column_def);

        Ok(())
    }
    fn visit_column_constraint(
        &self,
        node: Node,
        column_def: &mut ColumnDef,
        index_map: &mut HashMap<String, AttrIndex>,
    ) -> RS<()> {
        if node
            .child_by_field_name(ts_field_name::PRIMARY_KEY)
            .is_some()
        {
            let next_index = index_map
                .entry(ts_field_name::PRIMARY_KEY.to_string())
                .or_insert(0);
            column_def.set_primary_key_index(Some(*next_index));
            *next_index += 1;
        }
        if node_or_descendant_has_kind(node, ts_kind_name::S__NOT_NULL)
            || (node_or_descendant_has_kind(node, ts_kind_name::S_KEYWORD_NOT)
                && node_or_descendant_has_kind(node, ts_kind_name::S_KEYWORD_NULL))
        {
            column_def.set_nullable(false);
        }
        Ok(())
    }

    fn visit_data_type(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(UniDatType, Option<Vec<UniDatValue>>)> {
        let opt = node.child_by_field_name(ts_field_name::DATA_TYPE_KIND);
        let n = rs_option(opt, "")?;
        self.visit_data_type_kind(context, n)
    }

    fn visit_data_type_kind(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(UniDatType, Option<Vec<UniDatValue>>)> {
        let opt_n = node.child(0);
        let child = rs_option(opt_n, "no child in data type kind")?;
        let kind = child.kind_id();
        let ret = match kind {
            ts_kind_id::INT => (UniDatType::Scalar(UniScalar::I32), None),
            ts_kind_id::BIGINT => (UniDatType::Scalar(UniScalar::I64), None),
            ts_kind_id::HUGEINT => (UniDatType::Scalar(UniScalar::I128), None),
            ts_kind_id::DOUBLE => (UniDatType::Scalar(UniScalar::F64), None),
            ts_kind_id::FLOAT => (UniDatType::Scalar(UniScalar::F32), None),
            ts_kind_id::CHAR | ts_kind_id::VARCHAR | ts_kind_id::KEYWORD_TEXT => {
                let opt_params = if kind == ts_kind_id::CHAR || kind == ts_kind_id::VARCHAR {
                    let param = self.visit_char_param(context, child)?;
                    Some(vec![param])
                } else {
                    None
                };
                (UniDatType::Scalar(UniScalar::String), opt_params)
            }
            ts_kind_id::NUMERIC | ts_kind_id::DECIMAL => (
                UniDatType::Scalar(UniScalar::Numeric),
                self.visit_numeric_params(context, child)?,
            ),
            ts_kind_id::KEYWORD_DATE => (UniDatType::Scalar(UniScalar::Date), None),
            ts_kind_id::TIME => (
                UniDatType::Scalar(UniScalar::Time),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIME => (
                UniDatType::Scalar(UniScalar::Time),
                self.visit_optional_precision_param(context, node)?,
            ),
            ts_kind_id::TIMESTAMP => (
                UniDatType::Scalar(UniScalar::Timestamp),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIMESTAMP_BASE => (
                UniDatType::Scalar(UniScalar::Timestamp),
                self.visit_optional_precision_param(context, node)?,
            ),
            ts_kind_id::TIMESTAMPTZ => (
                UniDatType::Scalar(UniScalar::TimestampTz),
                self.visit_optional_precision_param(context, child)?,
            ),
            ts_kind_id::KEYWORD_TIMESTAMPTZ_BASE => (
                UniDatType::Scalar(UniScalar::TimestampTz),
                self.visit_optional_precision_param(context, node)?,
            ),
            _ => {
                return Err(m_error!(
                    EC::NotImplemented,
                    format!("Data type {} not yet implemented", child.kind())
                ));
            }
        };

        Ok(ret)
    }

    fn visit_char_param(&self, context: &ParseContext, node: Node) -> RS<UniDatValue> {
        if let Some(n) = node.child_by_field_name(ts_field_name::LENGTH) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let r = i64::from_str(s.as_str());
            match r {
                Ok(l) => Ok(UniDatValue::Scalar(UniScalarValue::I64(l))),
                Err(e) => Err(m_error!(EC::ParseErr, "parse u32 error", e)),
            }
        } else {
            Err(m_error!(EC::NoneErr, "No child parameter found"))
        }
    }

    fn visit_numeric_params(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Option<Vec<UniDatValue>>> {
        let mut params = Vec::new();
        if let Some(n) = node.child_by_field_name(ts_field_name::PRECISION) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| m_error!(EC::ParseErr, "parse precision error", e))?;
            params.push(UniDatValue::Scalar(UniScalarValue::I64(value)));
        }
        if let Some(n) = node.child_by_field_name(ts_field_name::SCALE) {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| m_error!(EC::ParseErr, "parse scale error", e))?;
            params.push(UniDatValue::Scalar(UniScalarValue::I64(value)));
        }
        if params.is_empty() {
            Ok(None)
        } else {
            Ok(Some(params))
        }
    }

    fn visit_optional_precision_param(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Option<Vec<UniDatValue>>> {
        let opt = node
            .child_by_field_name(ts_field_name::PRECISION)
            .or_else(|| node.child_by_field_name(ts_field_name::SIZE));
        if let Some(n) = opt {
            let s = ts_node_context_string(context.parse_str(), &n)?;
            let value = i64::from_str(s.as_str())
                .map_err(|e| m_error!(EC::ParseErr, "parse precision error", e))?;
            Ok(Some(vec![UniDatValue::Scalar(UniScalarValue::I64(value))]))
        } else {
            Ok(None)
        }
    }

    fn visit_insert_statement(&self, context: &ParseContext, node: Node) -> RS<StmtInsert> {
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let c = rs_option(opt, "no object reference in insert statement")?;
        let table_name = self.visit_object_reference(context, c)?;

        let opt = node.child_by_field_name(ts_field_name::INSERT_VALUES);
        let c = rs_option(opt, "no insert values clause in insert statement")?;
        let (columns, values) = self.visit_insert_values(context, c)?;
        let stmt = StmtInsert::new(table_name, columns, values);
        Ok(stmt)
    }

    fn expected_expr_value(expr: ExprType) -> RS<ExprValue> {
        match expr {
            ExprType::Value(v) => match &*v {
                ExprItem::ItemValue(expr_v) => match expr_v {
                    ExprValue::ValueLiteral(v) => Ok(ExprValue::ValueLiteral(v.clone())),
                    ExprValue::ValuePlaceholder => Ok(ExprValue::ValuePlaceholder),
                },
                _ => Err(m_error!(EC::TypeErr)),
            },
            _ => Err(m_error!(EC::TypeErr)),
        }
    }

    fn expected_expr_literal_vec(exprs: Vec<ExprType>) -> RS<Vec<ExprValue>> {
        let mut vec = vec![];
        for e in exprs {
            let el = Self::expected_expr_value(e)?;
            vec.push(el);
        }
        Ok(vec)
    }

    fn visit_typed_row_value_expr_list(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<Vec<Vec<ExprValue>>> {
        let mut cursor = node.walk();
        let mut value_expr_list = vec![];
        let iter = node.children_by_field_name(ts_field_name::LIST, &mut cursor);
        for c in iter {
            let expr_list = self.visit_list(context, c)?;
            let expr_literal = Self::expected_expr_literal_vec(expr_list)?;
            value_expr_list.push(expr_literal);
        }
        Ok(value_expr_list)
    }

    fn visit_insert_values(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<(Vec<String>, Vec<Vec<ExprValue>>)> {
        let opt = node.child_by_field_name(ts_field_name::COLUMN_LIST);
        let mut columns = vec![];
        if let Some(c) = opt {
            let mut f = |name: String| {
                columns.push(name);
                Ok::<_, MError>(())
            };
            self.visit_column_list(context, c, &mut f)?;
        }

        let opt = node.child_by_field_name(ts_field_name::TYPED_ROW_VALUE_EXPR_LIST);
        let n_val_expr_list = rs_of_opt(opt, || {
            m_error!(
                EC::ParseErr,
                format!(
                    "no value expression list node {}",
                    ts_node_context_string(context.parse_str(), &node).unwrap()
                )
            )
        })?;
        let expr_l = self.visit_typed_row_value_expr_list(context, n_val_expr_list)?;
        Ok((columns, expr_l))
    }

    fn visit_column_list<F>(&self, context: &ParseContext, node: Node, f: &mut F) -> RS<()>
    where
        F: FnMut(String) -> RS<()>,
    {
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::COLUMN, &mut cursor);
        for c in iter {
            let column_name = self.visit_column(context, c)?;
            f(column_name)?;
        }
        Ok(())
    }

    fn visit_list(&self, context: &ParseContext, node: Node) -> RS<Vec<ExprType>> {
        let mut vec = vec![];
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::EXPRESSION, &mut cursor);
        for n in iter {
            let expr = self.visit_expression(context, n)?;
            vec.push(expr);
        }
        Ok(vec)
    }

    fn visit_column(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    fn visit_update_statement(&self, context: &ParseContext, node: Node) -> RS<StmtUpdate> {
        let mut stmt = StmtUpdate::new();

        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference = rs_option(opt, "")?;
        let table_reference = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(table_reference);

        let opt = node.child_by_field_name(ts_field_name::SET_VALUES);
        let n_set_values = rs_option(opt, "no set values clause in update statement")?;
        let set_values = self.visit_set_values(context, n_set_values)?;
        stmt.set_set_values(set_values);

        let opt = node.child_by_field_name(ts_field_name::WHERE);
        let n_where = rs_option(opt, "no where clause in update statement")?;
        let expr_list = self.visit_where(context, n_where)?;
        stmt.set_where_predicate(expr_list);

        Ok(stmt)
    }

    fn visit_delete_statement(&self, context: &ParseContext, node: Node) -> RS<StmtDelete> {
        let mut stmt = StmtDelete::new();
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference = rs_option(opt, "no object reference in delete statement")?;
        let table_reference = self.visit_object_reference(context, n_object_reference)?;
        stmt.set_table_reference(table_reference);
        let opt = node.child_by_field_name(ts_field_name::WHERE);
        let n_where = rs_option(opt, "no where clause in delete statement")?;
        let expr_list = self.visit_where(context, n_where)?;
        stmt.set_where_predicate(expr_list);
        Ok(stmt)
    }

    fn visit_set_values(&self, context: &ParseContext, node: Node) -> RS<Vec<Assignment>> {
        let mut cursor = node.walk();
        let mut set_values = vec![];
        let iter = node.children_by_field_name(ts_field_name::ASSIGNMENT, &mut cursor);
        for n in iter {
            let assignment = self.visit_assignment(context, n)?;
            set_values.push(assignment);
        }
        Ok(set_values)
    }

    fn visit_assignment(&self, context: &ParseContext, node: Node) -> RS<Assignment> {
        let opt = node.child_by_field_name(ts_field_name::LEFT);
        let n_left = rs_option(opt, "no left in assignment node")?;
        let column_reference = self.visit_field(context, n_left)?;

        let opt = node.child_by_field_name(ts_field_name::RIGHT);
        let n_right = rs_option(opt, "no right in assignment node")?;
        let expr = self.visit_expression(context, n_right)?;
        let expr_l = match &expr {
            ExprType::Value(value) => match &(**value) {
                ExprItem::ItemValue(value) => AssignedValue::Value(value.clone()),
                _ => AssignedValue::Expression(expr),
            },
            _ => AssignedValue::Expression(expr),
        };

        let assignment = Assignment::new(column_reference, expr_l);
        Ok(assignment)
    }

    fn visit_field(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }
}

fn starts_with_ignore_ascii_case(input: &str, prefix: &str) -> bool {
    input
        .get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

fn contains_ignore_ascii_case(input: &str, needle: &str) -> bool {
    input
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

fn find_keyword_position(input: &str, keyword: &str) -> Option<usize> {
    let lower = input.to_ascii_lowercase();
    lower.find(&keyword.to_ascii_lowercase())
}

fn find_matching_paren(input: &str, open_index: usize) -> RS<usize> {
    let bytes = input.as_bytes();
    let mut depth = 0usize;
    let mut in_single_quote = false;
    for (index, byte) in bytes.iter().enumerate().skip(open_index) {
        match *byte {
            b'\'' => in_single_quote = !in_single_quote,
            b'(' if !in_single_quote => depth += 1,
            b')' if !in_single_quote => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(index);
                }
            }
            _ => {}
        }
    }
    Err(m_error!(EC::ParseErr, "unbalanced parentheses"))
}

fn split_top_level_csv(input: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_single_quote = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '\'' => in_single_quote = !in_single_quote,
            '(' if !in_single_quote => depth += 1,
            ')' if !in_single_quote => depth = depth.saturating_sub(1),
            ',' if !in_single_quote && depth == 0 => {
                items.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    items
}

fn parse_table_partition_suffix(input: &str) -> RS<StmtTablePartition> {
    let prefix = "partition by global rule ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(m_error!(
            EC::ParseErr,
            "expected PARTITION BY GLOBAL RULE clause"
        ));
    }
    let rest = input[prefix.len()..].trim();
    let references_pos = find_keyword_position(rest, "references")
        .ok_or_else(|| m_error!(EC::ParseErr, "partition clause must contain REFERENCES"))?;
    let rule_name = rest[..references_pos].trim();
    let refs = rest[references_pos + "references".len()..].trim();
    if !refs.starts_with('(') {
        return Err(m_error!(
            EC::ParseErr,
            "REFERENCES clause must be wrapped in parentheses"
        ));
    }
    let close_index = find_matching_paren(refs, 0)?;
    let cols = split_top_level_csv(&refs[1..close_index])
        .into_iter()
        .map(|col| col.trim().to_string())
        .filter(|col| !col.is_empty())
        .collect::<Vec<_>>();
    if rule_name.is_empty() || cols.is_empty() {
        return Err(m_error!(EC::ParseErr, "invalid table partition clause"));
    }
    Ok(StmtTablePartition::new(rule_name.to_string(), cols))
}

fn parse_range_partition_def(input: &str) -> RS<StmtRangePartition> {
    let prefix = "partition ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(m_error!(
            EC::ParseErr,
            format!("invalid partition definition {}", input)
        ));
    }
    let rest = input[prefix.len()..].trim();
    let values_pos = find_keyword_position(rest, "values")
        .ok_or_else(|| m_error!(EC::ParseErr, "partition definition must contain VALUES"))?;
    let name = rest[..values_pos].trim();
    let after_values = rest[values_pos + "values".len()..].trim();
    if !starts_with_ignore_ascii_case(after_values, "from") {
        return Err(m_error!(
            EC::ParseErr,
            "partition definition must contain FROM"
        ));
    }
    let after_from = after_values["from".len()..].trim();
    let from_close = find_matching_paren(after_from, 0)?;
    let start = parse_partition_bound(&after_from[..=from_close])?;
    let after_start = after_from[from_close + 1..].trim();
    if !starts_with_ignore_ascii_case(after_start, "to") {
        return Err(m_error!(
            EC::ParseErr,
            "partition definition must contain TO"
        ));
    }
    let after_to = after_start["to".len()..].trim();
    let end_close = find_matching_paren(after_to, 0)?;
    let end = parse_partition_bound(&after_to[..=end_close])?;
    Ok(StmtRangePartition::new(name.to_string(), start, end))
}

fn parse_partition_bound(input: &str) -> RS<StmtPartitionBound> {
    let trimmed = input.trim();
    if !trimmed.starts_with('(') || !trimmed.ends_with(')') {
        return Err(m_error!(
            EC::ParseErr,
            "partition bound must be parenthesized"
        ));
    }
    let items = split_top_level_csv(&trimmed[1..trimmed.len() - 1]);
    if items.len() == 1
        && (items[0].eq_ignore_ascii_case("minvalue") || items[0].eq_ignore_ascii_case("maxvalue"))
    {
        return Ok(StmtPartitionBound::Unbounded);
    }
    let mut values = Vec::with_capacity(items.len());
    for item in items {
        if item.eq_ignore_ascii_case("minvalue") || item.eq_ignore_ascii_case("maxvalue") {
            return Ok(StmtPartitionBound::Unbounded);
        }
        values.push(item.trim().as_bytes().to_vec());
    }
    Ok(StmtPartitionBound::Value(values))
}

fn parse_partition_placement_item(input: &str) -> RS<StmtPartitionPlacementItem> {
    let prefix = "partition ";
    if !starts_with_ignore_ascii_case(input, prefix) {
        return Err(m_error!(
            EC::ParseErr,
            format!("invalid partition placement item {}", input)
        ));
    }
    let rest = input[prefix.len()..].trim();
    let on_worker = find_keyword_position(rest, "on worker").ok_or_else(|| {
        m_error!(
            EC::ParseErr,
            "partition placement item must contain ON WORKER"
        )
    })?;
    let partition_name = rest[..on_worker].trim();
    let worker_id = rest[on_worker + "on worker".len()..].trim();
    if partition_name.is_empty() || worker_id.is_empty() {
        return Err(m_error!(
            EC::ParseErr,
            format!("invalid partition placement item {}", input)
        ));
    }
    Ok(StmtPartitionPlacementItem::new(
        partition_name.to_string(),
        worker_id.to_string(),
    ))
}

fn ts_node_context_string(s: &str, n: &Node) -> RS<String> {
    let ret = s.substring(n.start_byte(), n.end_byte());
    Ok(ret.to_string())
}
