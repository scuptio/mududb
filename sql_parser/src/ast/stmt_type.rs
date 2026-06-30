use crate::ast::stmt_copy_from::StmtCopyFrom;
use crate::ast::stmt_copy_to::StmtCopyTo;
use crate::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use crate::ast::stmt_create_partition_rule::StmtCreatePartitionRule;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ast::stmt_insert::StmtInsert;
use crate::ast::stmt_select::StmtSelect;
use crate::ast::stmt_update::StmtUpdate;

/// Top-level parsed SQL statement type.
#[derive(Clone, Debug)]
pub enum StmtType {
    /// `SELECT` statement.
    Select(StmtSelect),
    /// Command statement (DML or DDL).
    Command(StmtCommand),
}

/// SQL command statement variants.
#[derive(Clone, Debug)]
pub enum StmtCommand {
    /// `UPDATE` statement.
    Update(StmtUpdate),
    /// `DELETE` statement.
    Delete(StmtDelete),
    /// `CREATE PARTITION PLACEMENT` statement.
    CreatePartitionPlacement(StmtCreatePartitionPlacement),
    /// `INSERT` statement.
    Insert(StmtInsert),
    /// `CREATE PARTITION RULE` statement.
    CreatePartitionRule(StmtCreatePartitionRule),
    /// `CREATE TABLE` statement.
    CreateTable(StmtCreateTable),
    /// `DROP TABLE` statement.
    DropTable(StmtDropTable),
    /// `COPY ... TO` statement.
    CopyTo(StmtCopyTo),
    /// `COPY ... FROM` statement.
    CopyFrom(StmtCopyFrom),
}
