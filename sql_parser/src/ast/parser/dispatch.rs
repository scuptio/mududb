//! Statement dispatch parser.

use super::context::ParseContext;
use super::SQLParser;
use crate::ast::stmt_copy_from::StmtCopyFrom;
use crate::ast::stmt_copy_to::StmtCopyTo;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use crate::ts_const::{ts_field_name, ts_kind_id};
use mudu::common::result::RS;
use mudu::common::result_of::{rs_of_opt, rs_option};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_statement_gut(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let kind = node.kind_id();
        match kind {
            ts_kind_id::DML_READ_STMT => self.visit_dml_read_stmt(context, node),
            ts_kind_id::DML_WRITE_STMT => self.visit_dml_write_stmt(context, node),
            ts_kind_id::DDL_STMT => self.visit_ddl_stmt(context, node),
            ts_kind_id::COPY_STMT => self.visit_copy_stmt(context, node),
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_dml_read_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::SELECT_STATEMENT => {
                let stmt = self.visit_select_statement(context, child)?;
                Ok(StmtType::Select(stmt))
            }
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_ddl_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
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
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_dml_write_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
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
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_copy_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let opt_child = node.child(0);
        let child = rs_option(opt_child, "")?;
        let kind = child.kind_id();
        match kind {
            ts_kind_id::COPY_FROM => self.visit_copy_from_stmt(context, child),
            ts_kind_id::COPY_TO => self.visit_copy_to_stmt(context, child),
            _ => Err(mudu_error!(ErrorCode::NotImplemented)),
        }
    }

    pub(crate) fn visit_copy_from_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        let n = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let n_obj_ref = rs_of_opt(n, || {
            mudu_error!(ErrorCode::Parse, "no object reference field")
        })?;
        let table_name = self.visit_object_reference(context, n_obj_ref)?;
        let n = node.child_by_field_name(ts_field_name::FILE_PATH);
        let n_file_path = rs_of_opt(n, || {
            mudu_error!(ErrorCode::Parse, "no object file path field")
        })?;
        let file_path = self.visit_string(context, n_file_path)?;
        let copy_from = StmtCopyFrom::new(file_path, table_name, vec![]);
        let st = StmtType::Command(StmtCommand::CopyFrom(copy_from));
        Ok(st)
    }

    pub(crate) fn visit_copy_to_stmt(&self, context: &ParseContext, node: Node) -> RS<StmtType> {
        // tree-sitter `copy_to` currently does not expose field names for children.
        let mut object_ref = node.child_by_field_name(ts_field_name::OBJECT_REFERENCE);
        let mut file_path = node.child_by_field_name(ts_field_name::FILE_PATH);
        for i in 0..node.child_count() {
            let Some(child) = node.child(i as _) else {
                continue;
            };
            if object_ref.is_none() && child.kind_id() == ts_kind_id::OBJECT_REFERENCE {
                object_ref = Some(child);
            } else if file_path.is_none() && child.kind_id() == ts_kind_id::FILE_PATH {
                file_path = Some(child);
            }
        }

        let n_obj_ref = rs_of_opt(object_ref, || {
            mudu_error!(ErrorCode::Parse, "no object reference field")
        })?;
        let table_name = self.visit_object_reference(context, n_obj_ref)?;
        let n_file_path = rs_of_opt(file_path, || {
            mudu_error!(ErrorCode::Parse, "no object file path field")
        })?;
        let file_path = self.visit_string(context, n_file_path)?;
        let copy_to = StmtCopyTo::new(file_path, table_name, vec![]);
        Ok(StmtType::Command(StmtCommand::CopyTo(copy_to)))
    }
}
