//! DDL (CREATE/DROP TABLE) statement parser.

use super::context::ParseContext;
use super::SQLParser;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ts_const::{ts_field_name, ts_kind_id};
use mudu::common::result::RS;
use mudu::common::result_of::rs_option;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_drop_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtDropTable> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::DROP_TABLE => {
                let s = self.visit_drop_table_statement(context, child)?;
                Ok(s)
            }
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_drop_table_statement(
        &self,
        context: &ParseContext,
        node: Node,
    ) -> RS<StmtDropTable> {
        let opt = node.child_by_field_name(ts_field_name::IF_EXIST);
        let if_exist = opt.is_some();
        let opt = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n = match opt {
            Some(n) => n,
            None => {
                return Err(mudu_error!(ErrorCode::Parse, "drop table statement"));
            }
        };
        let object = self.visit_object_reference(context, n)?;
        Ok(StmtDropTable::new(object, if_exist))
    }

    pub(crate) fn visit_create_table_statement(
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

    pub(crate) fn visit_column_definitions(
        &self,
        context: &ParseContext,
        node: Node,
        stmt: &mut StmtCreateTable,
    ) -> RS<()> {
        let n = node.child_count();
        for i in 0..n {
            let Some(c) = node.child(i as _) else {
                continue;
            };
            if c.kind_id() == ts_kind_id::COLUMN_DEFINITION {
                self.visit_column_definition(context, c, stmt)?;
            } else if c.kind_id() == ts_kind_id::CONSTRAINTS {
                self.visit_constraints(context, c, stmt)?;
            }
        }
        Ok(())
    }

    pub(crate) fn visit_constraints(
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

    pub(crate) fn visit_constraint(
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

    pub(crate) fn visit_primary_key_constraint(
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
                Err(mudu_error!(ErrorCode::EntityNotFound))
            }
        };
        self.visit_column_list(context, n, &mut f)?;
        Ok(())
    }
}
