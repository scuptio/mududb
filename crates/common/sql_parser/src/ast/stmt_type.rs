use crate::ast::stmt_ack::StmtAck;
use crate::ast::stmt_copy_from::StmtCopyFrom;
use crate::ast::stmt_copy_to::StmtCopyTo;
use crate::ast::stmt_create_cache::StmtCreateCache;
use crate::ast::stmt_create_consumer::StmtCreateConsumer;
use crate::ast::stmt_create_fs_type::StmtCreateFsType;
use crate::ast::stmt_create_partition_placement::StmtCreatePartitionPlacement;
use crate::ast::stmt_create_partition_rule::StmtCreatePartitionRule;
use crate::ast::stmt_create_queue::StmtCreateQueue;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_create_type::StmtCreateType;
use crate::ast::stmt_delete::StmtDelete;
use crate::ast::stmt_drop_consumer::StmtDropConsumer;
use crate::ast::stmt_drop_queue::StmtDropQueue;
use crate::ast::stmt_drop_table::StmtDropTable;
use crate::ast::stmt_drop_type::StmtDropType;
use crate::ast::stmt_insert::StmtInsert;
use crate::ast::stmt_receive::StmtReceive;
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
    /// `CREATE TYPE FILESYSTEM FILE|DIRECTORY` statement.
    CreateFsType(StmtCreateFsType),
    /// `CREATE TYPE ... AS (...)` statement (transient type declaration).
    CreateType(StmtCreateType),
    /// `CREATE CACHE ... LIKE ...` statement (volatile cache declaration).
    CreateCache(StmtCreateCache),
    /// `DROP TYPE` statement.
    DropType(StmtDropType),
    /// `COPY ... TO` statement.
    CopyTo(StmtCopyTo),
    /// `COPY ... FROM` statement.
    CopyFrom(StmtCopyFrom),
    /// `CREATE QUEUE` statement.
    CreateQueue(StmtCreateQueue),
    /// `DROP QUEUE` statement.
    DropQueue(StmtDropQueue),
    /// `RECEIVE` statement.
    Receive(StmtReceive),
    /// `ACK` statement.
    Ack(StmtAck),
    /// `CREATE CONSUMER` statement.
    CreateConsumer(StmtCreateConsumer),
    /// `DROP CONSUMER` statement.
    DropConsumer(StmtDropConsumer),
}
