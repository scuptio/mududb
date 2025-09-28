use mudu::common::result_of::{rs_of_opt, rs_option};

use mudu::common::result::RS;
use mudu::error::ec::EC;

use crate::ts_const::{ts_field_name, ts_kind_id};
use std::collections::HashMap;
use std::f64;
use std::io::Write;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use substring::Substring;
use tree_sitter::{Language, Node, Parser};
use tree_sitter_sql;

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
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ast::stmt_insert::StmtInsert;
use crate::ast::stmt_list::StmtList;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ast::stmt_update::{AssignedValue, Assignment, StmtUpdate};
use mudu::data_type::dat_type::DatType;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::dt_impl::dat_typed::DatTyped;
use mudu::data_type::param_info::ParamInfo;
use mudu::error::err::MError;
use mudu::m_error;

pub struct SQLParser {
    parser: Mutex<Parser>,
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
    tree_sitter_sql::LANGUAGE.clone().into()
}


fn traverse_tree_for_error_nodes<'t>(node: &Node<'t>, error_nodes: &mut Vec<Node<'t>>) {
    if !node.has_error() {
        return;
    }

    if node.kind() == "ERROR" || node.is_missing() {
        error_nodes.push(node.clone());
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree_for_error_nodes(&child, error_nodes);
    }
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
            } else if i != line_end && i == line_end {
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
fn print_error_line<W: Write>(
    parse_text: &str,
    node: Node,
    writter: &mut W,
) -> RS<()> {
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

    let error_msg = format!("In \
        position: [{},{}; {},{}], \
        text: [{}]
        child tokens:[{}], \
        parent kind:[{}],\
        s-expr: [{}]\n",
                            line_start, column_start, line_end, column_end,
                            error_text,
                            tokens,
                            kind,
                            node.to_sexp());

    writter.write_fmt(format_args!("{}", error_msg)).unwrap();
    Ok(())
}

fn print_parse_error<W: Write>(parse_text: &str, node: &Node, writer: &mut W) -> RS<()> {
    let mut error_nodes = vec![];
    traverse_tree_for_error_nodes(node, &mut error_nodes);
    for node in error_nodes {
        print_error_line(
            parse_text,
            node,
            writer,
        )?
    }
    Ok(())
}


impl SQLParser {
    pub fn new() -> SQLParser {
        let mut parser = Parser::new();
        parser.set_language(&sql_language()).unwrap();
        Self {
            parser: Mutex::new(parser),
        }
    }

    pub fn parse(&self, sql: &str) -> RS<StmtList> {
        let parse_context = ParseContext::new(sql.to_string());
        let mut guard = self.parser.lock().unwrap();
        let opt_tree = guard.parse(sql, None);
        let tree = match opt_tree {
            Some(tree) => { tree }
            None => { return Err(m_error!(EC::MLParseError, "SQL parse error")) }
        };
        let vec = self.visit_root(&parse_context, tree.root_node())?;
        let stmt = StmtList::new(vec);
        Ok(stmt)
    }

    fn parse_error(&self, context: &ParseContext, node: &Node) -> RS<()> {
        if node.has_error() {
            let mut buffer = Vec::new();
            print_parse_error(context.parse_str(), node, &mut buffer)?;
            let error = String::from_utf8(buffer).map_err(|e| {
                m_error!(EC::MuduError, "", e)
            })?;
            Err(m_error!(EC::MLParseError, format!(
                "Syntax error at position start {}, end {}, at text\n\
                 \"\n\
                 {}\n\",\
                 \nErrors, {}",
                node.start_position(),
                node.end_position(),
                ts_node_context_string(context.parse_str(), node)?,
                error
            )))
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
            let child = node.child(i).unwrap();
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
        let n_obj_ref = rs_of_opt(n, || {
            m_error!(EC::ParseErr, "no object reference field")
        })?;
        let table_name = self.visit_object_reference(context, n_obj_ref)?;
        let n = node.child_by_field_name(ts_field_name::FILE_PATH);
        let n_file_path = rs_of_opt(n, || {
            m_error!(EC::ParseErr, "no object file path field")
        })?;
        let file_path = self.visit_string(context, n_file_path)?;
        let copy_from = StmtCopyFrom::new(file_path, table_name, vec![]);
        let st = StmtType::Command(StmtCommand::CopyFrom(copy_from));
        Ok(st)
    }

    fn visit_copy_to_stmt(&self, _context: &ParseContext, _node: Node) -> RS<StmtType> {
        todo!()
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

    fn visit_where_predicate_expression(&self, context: &ParseContext, node: Node) -> RS<Vec<ExprCompare>> {
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
            return Ok(ExprType::Value(Arc::new(
                ExprItem::ItemValue(ExprValue::ValueLiteral(literal)))));
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
            return Ok(ExprType::Value(Arc::new(ExprItem::ItemValue(ExprValue::ValuePlaceholder))));
        }
        panic!(
            "unknown expression {}",
            ts_node_context_string(&context.parse_str(), &node)?
        )
    }

    fn visit_literal(&self, context: &ParseContext, node: Node) -> RS<ExprLiteral> {
        let typed = if let Some(n) = node.child_by_field_name("integer") {
            let s = self.visit_integer(context, n)?;
            let i = i64::from_str(s.as_str()).unwrap();
            DatTyped::I64(i)
        } else if let Some(n) = node.child_by_field_name("decimal") {
            let s = self.visit_decimal(context, n)?;
            let f = f64::from_str(s.as_str()).unwrap();
            DatTyped::F64(f)
        } else if let Some(n) = node.child_by_field_name("string") {
            let s = self.visit_string(context, n)?;
            DatTyped::String(s)
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
        let op = Operator::from_str(op_string);
        Ok(op)
    }

    fn visit_relation(&self, context: &ParseContext, node: Node, stmt: &mut StmtSelect) -> RS<()> {
        let opt_n_object_reference = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_object_reference = rs_option(opt_n_object_reference, "no object reference in relation")?;
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

    fn visit_select_expression(&self, context: &ParseContext, node: Node, stmt: &mut StmtSelect) -> RS<()> {
        for i in 0..node.child_count() {
            let n = node.child(i).unwrap();
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

    fn visit_projection_expression(&self, context: &ParseContext, node: Node, term: &mut SelectTerm) -> RS<()> {
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
            None => Err(m_error!(EC::NoneErr,
                format!("alias not found in {}",
                        ts_node_context_string(&context.parse_str(), &node)?))),
            Some(n) => {
                let s = ts_node_context_string(&context.parse_str(), &n)?;
                Ok(s)
            }
        }
    }

    fn visit_identifier(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(&context.parse_str(), &node)
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

    fn visit_create_table_statement(&self, context: &ParseContext, node: Node) -> RS<StmtCreateTable> {
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

    fn visit_column_definitions(&self, context: &ParseContext, node: Node, stmt: &mut StmtCreateTable) -> RS<()> {
        let n = node.child_count();
        for i in 0..n {
            let c = node.child(i).unwrap();
            if c.kind_id() == ts_kind_id::COLUMN_DEFINITION {
                self.visit_column_definition(context, c, stmt)?;
            } else if c.kind_id() == ts_kind_id::CONSTRAINTS {
                self.visit_constraints(context, c, stmt)?;
            }
        }
        Ok(())
    }

    fn visit_constraints(&self, context: &ParseContext, node: Node, stmt: &mut StmtCreateTable) -> RS<()> {
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::CONSTRAINT, &mut cursor);
        for n in iter {
            self.visit_constraint(context, n, stmt)?;
        }

        Ok(())
    }

    fn visit_constraint(&self, context: &ParseContext, node: Node, stmt: &mut StmtCreateTable) -> RS<()> {
        if let Some(n) = node.child_by_field_name(ts_field_name::PRIMARY_KEY_CONSTRAINT) {
            self.visit_primary_key_constraint(context, n, stmt)?;
        }

        Ok(())
    }

    fn visit_primary_key_constraint(&self, context: &ParseContext, node: Node, stmt: &mut StmtCreateTable) -> RS<()> {
        let opt_n = node.child_by_field_name(ts_field_name::COLUMN_LIST);

        let n = rs_option(opt_n, "no column list in primary key constraint")?;
        let mut map = HashMap::new();
        for d in stmt.mutable_column_def().iter_mut() {
            map.insert(d.column_name().clone(), d);
        }
        let mut index = 0;
        let mut f = |name: String| {
            if let Some(n) = map.get_mut(&name) {
                n.set_primary_key(true);
                n.set_index(index);
                index += 1;
                Ok(())
            } else {
                Err(m_error!(EC::NoSuchElement))
            }
        };
        self.visit_column_list(context, n, &mut f)?;
        Ok(())
    }

    fn visit_column_definition(&self, context: &ParseContext, node: Node, stmt: &mut StmtCreateTable) -> RS<()> {
        let opt_n = node.child_by_field_name(ts_field_name::COLUMN_NAME);
        let n_column_name = rs_option(opt_n, "")?;
        let column_name = self.visit_identifier(context, n_column_name)?;

        let opt_n = node.child_by_field_name(ts_field_name::DATA_TYPE);
        let n_data_type = rs_option(opt_n, "")?;
        let type_def = self.visit_data_type(context, n_data_type)?;
        let mut column_def = ColumnDef::new(column_name, type_def, false);
        let mut cursor = node.walk();
        let iter = node.children_by_field_name(ts_field_name::COLUMN_CONSTRAINT, &mut cursor);
        for n in iter {
            self.visit_column_constraint(n, &mut column_def)?;
        }

        stmt.add_column_def(column_def);

        Ok(())
    }
    fn visit_column_constraint(&self, node: Node, column_def: &mut ColumnDef) -> RS<()> {
        if node.child_by_field_name(ts_field_name::PRIMARY_KEY).is_some() {
            column_def.set_primary_key(true);
        }
        Ok(())
    }

    fn visit_data_type(&self, context: &ParseContext, node: Node) -> RS<DatType> {
        let opt = node.child_by_field_name(ts_field_name::DATA_TYPE_KIND);
        let n = rs_option(opt, "")?;
        self.visit_data_type_kind(context, n)
    }

    fn visit_data_type_kind(&self, context: &ParseContext, node: Node) -> RS<DatType> {
        let opt_n = node.child(0);
        let child = rs_option(opt_n, "no child in data type kind")?;
        let kind = child.kind_id();
        let type_declare = match kind {
            ts_kind_id::INT => DatType::new_with_no_param(DatTypeID::I32),
            ts_kind_id::BIGINT => DatType::new_with_no_param(DatTypeID::I64),
            ts_kind_id::DOUBLE => DatType::new_with_no_param(DatTypeID::F64),
            ts_kind_id::FLOAT => DatType::new_with_no_param(DatTypeID::F32),
            ts_kind_id::CHAR | ts_kind_id::VARCHAR => {
                let data_type_id = if kind == ts_kind_id::CHAR {
                    DatTypeID::CharFixedLen
                } else {
                    DatTypeID::CharVarLen
                };
                let params = self.visit_char_param(context, child)?;
                let info = ParamInfo {
                    id: data_type_id,
                    ty_param: params,
                };
                let dt_param = info.to_object();
                DatType::new_with_obj(dt_param)
            }
            ts_kind_id::NUMERIC => DatType::new_with_no_param(DatTypeID::F64),
            ts_kind_id::DECIMAL => DatType::new_with_no_param(DatTypeID::F64),
            ts_kind_id::KEYWORD_TIMESTAMP => DatType::new_with_no_param(DatTypeID::I64),
            _ => return Err(m_error!(EC::NotImplemented, format!("Data type {} not yet implemented", child.kind()))),
        };

        Ok(type_declare)
    }

    fn visit_char_param(&self, context: &ParseContext, node: Node) -> RS<Vec<String>> {
        if let Some(n) = node.child_by_field_name(ts_field_name::LENGTH) {
            let s = ts_node_context_string(&context.parse_str(), &n)?;
            let r = u32::from_str(s.as_str());
            match r {
                Ok(l) => Ok(vec![l.to_string()]),
                Err(e) => Err(m_error!(EC::ParseErr, "parse u32 error", e)),
            }
        } else {
            Err(m_error!(EC::NoneErr, "No child parameter found"))
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
                ExprItem::ItemValue(expr_v) => {
                    match expr_v {
                        ExprValue::ValueLiteral(v) => Ok(ExprValue::ValueLiteral(v.clone())),
                        ExprValue::ValuePlaceholder => Ok(ExprValue::ValuePlaceholder)
                    }
                }
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

    fn visit_typed_row_value_expr_list(&self, context: &ParseContext, node: Node) -> RS<Vec<Vec<ExprValue>>> {
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

    fn visit_insert_values(&self, context: &ParseContext, node: Node) -> RS<(Vec<String>, Vec<Vec<ExprValue>>)> {
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
            m_error!(EC::ParseErr, format!(
                "no value expression list node {}",
                ts_node_context_string(&context.parse_str(), &node).unwrap()
            ))
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
        ts_node_context_string(&context.parse_str(), &node)
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
            ExprType::Value(value) => {
                match &(**value) {
                    ExprItem::ItemValue(value) => {
                        AssignedValue::Value(value.clone())
                    }
                    _ => { AssignedValue::Expression(expr) }
                }
            }
            _ => { AssignedValue::Expression(expr) }
        };

        let assignment = Assignment::new(column_reference, expr_l);
        Ok(assignment)
    }

    fn visit_field(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }
}

fn ts_node_context_string(s: &str, n: &Node) -> RS<String> {
    let ret = s.substring(n.start_byte(), n.end_byte());
    Ok(ret.to_string())
}
